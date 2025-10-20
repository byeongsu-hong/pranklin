use crate::{StateError, StateKey};
use alloy_primitives::B256;
use jmt::storage::{LeafNode, Node, NodeBatch, NodeKey, TreeReader, TreeWriter};
use jmt::{JellyfishMerkleTree, KeyHash, OwnedValue, Version};
use rocksdb::{DB, Options};
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
use std::sync::{Arc, RwLock};

/// RocksDB-backed storage for JMT with snapshot and pruning support
///
/// This implements both TreeReader and TreeWriter traits from the JMT crate,
/// allowing it to be used directly as storage backend for JellyfishMerkleTree.
///
/// **Architecture**:
/// - All state updates go through JMT (Jellyfish Merkle Tree)
/// - `set()` stores updates in memory buffer
/// - `commit()` writes buffered updates through JMT to RocksDB
/// - JMT manages the sparse merkle tree structure automatically
#[derive(Clone)]
pub struct RocksDbStorage {
    /// RocksDB instance
    db: Arc<DB>,
    /// Current version (block height)
    current_version: Arc<RwLock<u64>>,
    /// State root cache
    root_cache: Arc<RwLock<std::collections::HashMap<u64, B256>>>,
    /// Pruning configuration
    pruning_config: PruningConfig,
    /// Pending updates buffer (key_hash -> value) for current version
    /// These will be committed to JMT on next commit()
    pending_updates: Arc<RwLock<std::collections::HashMap<KeyHash, Vec<u8>>>>,
}

/// Pruning configuration
#[derive(Debug, Clone)]
pub struct PruningConfig {
    /// Keep snapshots every N blocks
    pub snapshot_interval: u64,
    /// Prune versions older than N blocks
    pub prune_before: u64,
    /// Enabled pruning
    pub enabled: bool,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            snapshot_interval: 1000, // Keep a snapshot every 1000 blocks
            prune_before: 100000,    // Prune data older than 100k blocks
            enabled: true,
        }
    }
}

impl RocksDbStorage {
    /// Create a new RocksDB storage with Aptos JMT optimized settings
    pub fn new<P: AsRef<Path>>(path: P, pruning_config: PruningConfig) -> Result<Self, StateError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        // Parallelism settings
        opts.increase_parallelism(num_cpus::get() as i32);
        opts.set_max_background_jobs(6);

        // LSM tree compaction optimizations for JMT workload
        opts.optimize_level_style_compaction(512 * 1024 * 1024); // 512MB memtable
        opts.set_level_zero_file_num_compaction_trigger(4);
        opts.set_level_zero_slowdown_writes_trigger(20);
        opts.set_level_zero_stop_writes_trigger(36);

        // Write buffer settings - important for JMT's write-heavy workload
        opts.set_write_buffer_size(256 * 1024 * 1024); // 256MB write buffer
        opts.set_max_write_buffer_number(4);
        opts.set_min_write_buffer_number_to_merge(2);

        // Block-based table options with Aptos-style optimizations
        let mut block_opts = rocksdb::BlockBasedOptions::default();

        // Bloom filter for faster point lookups (critical for JMT node access)
        block_opts.set_bloom_filter(10.0, false); // 10 bits per key

        // Block cache - shared across all column families
        // Aptos typically uses large block cache for better read performance
        let cache_size = 1024 * 1024 * 1024; // 1GB cache
        let cache = rocksdb::Cache::new_lru_cache(cache_size);
        block_opts.set_block_cache(&cache);

        // Block size optimization for JMT nodes
        block_opts.set_block_size(16 * 1024); // 16KB blocks (good for tree nodes)

        // Enable index and filter blocks in cache
        block_opts.set_cache_index_and_filter_blocks(true);
        block_opts.set_pin_l0_filter_and_index_blocks_in_cache(true);

        opts.set_block_based_table_factory(&block_opts);

        // Compression settings - Aptos uses LZ4 for better speed/compression tradeoff
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        opts.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd); // Zstd for cold data

        // Target file size for levels (important for LSM tree performance)
        opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB
        opts.set_target_file_size_multiplier(2);

        // Max bytes for level base
        opts.set_max_bytes_for_level_base(512 * 1024 * 1024); // 512MB
        opts.set_max_bytes_for_level_multiplier(10.0);

        // Enable statistics for monitoring (optional)
        opts.enable_statistics();
        opts.set_stats_dump_period_sec(60);

        // Write ahead log (WAL) settings
        opts.set_max_total_wal_size(1024 * 1024 * 1024); // 1GB max WAL size

        let db = DB::open(&opts, path)
            .map_err(|e| StateError::StorageError(format!("Failed to open RocksDB: {}", e)))?;

        let db = Arc::new(db);

        // Load current version from disk (for crash recovery)
        let current_version = match db.get(b"__current_version__") {
            Ok(Some(bytes)) if bytes.len() == 8 => u64::from_le_bytes(bytes.try_into().unwrap()),
            _ => 0, // Fresh database
        };

        Ok(Self {
            db,
            current_version: Arc::new(RwLock::new(current_version)),
            root_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            pruning_config,
            pending_updates: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    /// Get a value at a specific version
    ///
    /// This reads from:
    /// 1. Pending updates buffer (for uncommitted writes)
    /// 2. JMT via TreeReader.get_value_option() (for committed writes)
    pub fn get<T: DeserializeOwned>(
        &self,
        key: &StateKey,
        version: u64,
    ) -> Result<Option<T>, StateError> {
        let key_hash = key.hash();

        // First check pending updates (uncommitted)
        if let Some(bytes) = self.pending_updates.read().unwrap().get(&key_hash) {
            let value: T = serde_json::from_slice(bytes).map_err(|e| {
                StateError::DeserializationError(format!("Failed to deserialize: {}", e))
            })?;
            return Ok(Some(value));
        }

        // Not in pending, query from JMT (committed data)
        match self.get_value_option(version, key_hash) {
            Ok(Some(bytes)) => {
                let value: T = serde_json::from_slice(&bytes).map_err(|e| {
                    StateError::DeserializationError(format!("Failed to deserialize: {}", e))
                })?;
                Ok(Some(value))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StateError::StorageError(format!(
                "Failed to get from JMT: {}",
                e
            ))),
        }
    }

    /// Set a value (buffered in memory until commit)
    ///
    /// This adds the update to the pending buffer. The actual write to RocksDB
    /// happens in `commit()` when all pending updates are written through JMT.
    pub fn set<T: Serialize>(&self, key: StateKey, value: T) -> Result<(), StateError> {
        // Serialize the value
        let bytes = serde_json::to_vec(&value)
            .map_err(|e| StateError::SerializationError(format!("Failed to serialize: {}", e)))?;

        // Convert StateKey to KeyHash for JMT
        let key_hash = key.hash();

        // Add to pending updates buffer
        self.pending_updates
            .write()
            .unwrap()
            .insert(key_hash, bytes);

        Ok(())
    }

    /// Store latest version for a key (optimization for O(1) lookup)
    fn store_latest_version(&self, key_hash: &KeyHash, version: u64) -> Result<(), StateError> {
        let key = format!("latest_version_{}", hex::encode(key_hash.0));
        let version_bytes = version.to_le_bytes();
        self.db.put(key.as_bytes(), version_bytes).map_err(|e| {
            StateError::StorageError(format!("Failed to store latest version: {}", e))
        })?;
        Ok(())
    }

    /// Get latest version for a key (optimization for O(1) lookup)
    fn get_latest_version(&self, key_hash: &KeyHash) -> Result<Option<u64>, StateError> {
        let key = format!("latest_version_{}", hex::encode(key_hash.0));
        match self.db.get(key.as_bytes()) {
            Ok(Some(bytes)) => {
                if bytes.len() == 8 {
                    let version = u64::from_le_bytes(bytes.try_into().unwrap());
                    Ok(Some(version))
                } else {
                    Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StateError::StorageError(format!(
                "Failed to get latest version: {}",
                e
            ))),
        }
    }

    /// Delete a key (buffered in memory until commit)
    ///
    /// In JMT, deletions are represented as `None` values.
    /// The actual deletion happens in `commit()` when written through JMT.
    pub fn delete(&self, key: StateKey) -> Result<(), StateError> {
        // Convert StateKey to KeyHash
        let key_hash = key.hash();

        // Remove from pending updates (if it was added but not committed)
        // This effectively deletes it before it's written
        self.pending_updates.write().unwrap().remove(&key_hash);

        // TODO: For keys that are already committed, we need to add them
        // with None value to pending updates. For now, we just remove from buffer.
        // Full deletion support requires tracking which keys to delete.

        Ok(())
    }

    /// Commit the current version and return state root
    ///
    /// This writes all pending updates through JMT to RocksDB:
    /// 1. Takes pending updates from buffer
    /// 2. Writes them through JMT.put_value_set() → generates NodeBatch
    /// 3. Writes NodeBatch through TreeWriter.write_node_batch() → RocksDB
    /// 4. Calculates and caches state root
    /// 5. Stores latest version for each key (O(1) lookup optimization)
    /// 6. Creates snapshot if at snapshot interval
    /// 7. Prunes old data if enabled
    pub fn commit(&self, version: u64) -> Result<B256, StateError> {
        // Get and clear pending updates
        let pending = {
            let mut updates = self.pending_updates.write().unwrap();
            let pending = updates.clone();
            updates.clear();
            pending
        };

        // If there are pending updates, write them through JMT
        if !pending.is_empty() {
            // Create JMT instance
            let tree = JellyfishMerkleTree::<_, sha2::Sha256>::new(self);

            // Convert pending updates to JMT format: Vec<(KeyHash, OwnedValue)>
            let value_set: Vec<(KeyHash, Option<OwnedValue>)> = pending
                .iter()
                .map(|(key_hash, value)| (*key_hash, Some(value.clone())))
                .collect();

            // Write value set through JMT
            // This generates a TreeUpdateBatch with all the JMT nodes to write
            let (_root_hash, tree_update_batch) =
                tree.put_value_set(value_set, version).map_err(|e| {
                    StateError::StorageError(format!("JMT put_value_set failed: {}", e))
                })?;

            // Write the node batch to RocksDB via TreeWriter
            // TreeUpdateBatch contains: node_batch, stale_node_index_batch, node_stats
            self.write_node_batch(&tree_update_batch.node_batch)
                .map_err(|e| StateError::StorageError(format!("TreeWriter write failed: {}", e)))?;

            // Store values separately for fast O(1) access
            // This is more efficient than traversing JMT nodes
            for (key_hash, value_bytes) in &pending {
                let value_key = format!("jmt_value_{}_{}", hex::encode(key_hash.0), version);
                self.db
                    .put(value_key.as_bytes(), value_bytes)
                    .map_err(|e| {
                        StateError::StorageError(format!("Failed to store value: {}", e))
                    })?;
            }

            // Store latest version for each key (OPTIMIZATION: O(1) lookup)
            for key_hash in pending.keys() {
                self.store_latest_version(key_hash, version)?;
            }
        }

        // Calculate and cache state root
        let root = self.calculate_state_root(version)?;
        self.root_cache.write().unwrap().insert(version, root);

        // Update current version in memory and persist to disk
        *self.current_version.write().unwrap() = version;
        self.db
            .put(b"__current_version__", version.to_le_bytes())
            .map_err(|e| StateError::StorageError(format!("Failed to persist version: {}", e)))?;

        // Create snapshot if needed
        if self.pruning_config.enabled
            && version.is_multiple_of(self.pruning_config.snapshot_interval)
        {
            self.create_snapshot(version)?;
        }

        // Prune old data if needed
        if self.pruning_config.enabled && version > self.pruning_config.prune_before {
            let prune_before_version = version - self.pruning_config.prune_before;
            self.prune_before_version(prune_before_version)?;
        }

        Ok(root)
    }

    /// Get state root at a specific version
    pub fn get_root(&self, version: u64) -> B256 {
        self.root_cache
            .read()
            .unwrap()
            .get(&version)
            .copied()
            .unwrap_or(B256::ZERO)
    }

    /// Create a snapshot at the current version
    fn create_snapshot(&self, version: u64) -> Result<(), StateError> {
        // Mark this version as a snapshot
        let snapshot_key = format!("snapshot_{}", version);
        let root = self.get_root(version);

        self.db
            .put(snapshot_key.as_bytes(), root.as_slice())
            .map_err(|e| StateError::StorageError(format!("Failed to create snapshot: {}", e)))?;

        log::info!("Created snapshot at version {}", version);
        Ok(())
    }

    /// Prune data before a specific version
    ///
    /// This removes old state data to save disk space while keeping snapshots
    /// for point-in-time recovery.
    ///
    /// Strategy:
    /// 1. Keep all snapshot versions (every N blocks)
    /// 2. Prune all non-snapshot versions before prune_before
    /// 3. Use batch deletion for efficiency
    fn prune_before_version(&self, prune_before: u64) -> Result<(), StateError> {
        // Calculate which versions are snapshot versions and should be kept
        let snapshots_to_keep: std::collections::HashSet<u64> = (0..=prune_before)
            .filter(|v| v % self.pruning_config.snapshot_interval == 0)
            .collect();

        log::info!(
            "Starting pruning: removing data before version {}, keeping {} snapshots",
            prune_before,
            snapshots_to_keep.len()
        );

        // Collect keys to delete
        let mut keys_to_delete = Vec::new();
        let mut pruned_count = 0;

        let iter = self.db.iterator(rocksdb::IteratorMode::Start);
        for item in iter {
            match item {
                Ok((key, _)) => {
                    // Skip snapshot metadata keys
                    if key.starts_with(b"snapshot_") {
                        continue;
                    }

                    // Check if this key has a version suffix
                    if key.len() >= 8 {
                        let key_version = u64::from_le_bytes([
                            key[key.len() - 8],
                            key[key.len() - 7],
                            key[key.len() - 6],
                            key[key.len() - 5],
                            key[key.len() - 4],
                            key[key.len() - 3],
                            key[key.len() - 2],
                            key[key.len() - 1],
                        ]);

                        // Prune if:
                        // 1. Version is before prune_before
                        // 2. Version is NOT a snapshot version
                        if key_version < prune_before && !snapshots_to_keep.contains(&key_version) {
                            keys_to_delete.push(key.to_vec());
                            pruned_count += 1;

                            // Batch delete every 10000 keys to avoid memory issues
                            if keys_to_delete.len() >= 10000 {
                                self.batch_delete(&keys_to_delete)?;
                                keys_to_delete.clear();
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Error iterating during pruning: {}", e);
                    continue;
                }
            }
        }

        // Delete remaining keys
        if !keys_to_delete.is_empty() {
            self.batch_delete(&keys_to_delete)?;
        }

        log::info!(
            "Pruning completed: removed {} keys before version {}",
            pruned_count,
            prune_before
        );

        // Trigger compaction to reclaim disk space
        // This is important - deletion marks keys as deleted but doesn't immediately
        // reclaim disk space. Compaction is needed to actually free up space.
        if pruned_count > 0 {
            log::info!("Triggering compaction to reclaim disk space...");
            self.db.compact_range::<&[u8], &[u8]>(None, None);
            log::info!("Compaction completed");
        }

        Ok(())
    }

    /// Batch delete keys efficiently
    fn batch_delete(&self, keys: &[Vec<u8>]) -> Result<(), StateError> {
        let mut batch = rocksdb::WriteBatch::default();

        for key in keys {
            batch.delete(key);
        }

        self.db
            .write(batch)
            .map_err(|e| StateError::StorageError(format!("Failed to batch delete keys: {}", e)))?;

        Ok(())
    }

    /// Calculate state root using Jellyfish Merkle Tree (JMT)
    ///
    /// This uses Penumbra's JMT implementation which provides:
    /// - **Sparse merkle tree**: Only stores non-empty nodes (efficient for large state spaces)
    /// - **Incremental updates**: Efficient O(log n) state transitions
    /// - **Authenticated proofs**: Can generate and verify proofs for any key-value pair
    /// - **Battle-tested**: Used in production by Penumbra Zone blockchain
    ///
    /// The JMT reads from storage via the TreeReader trait (implemented below).
    fn calculate_state_root(&self, version: u64) -> Result<B256, StateError> {
        // Create JMT instance with self as the storage backend
        // The TreeReader trait implementation below provides read access to JMT nodes
        // Using sha2::Sha256 as the hasher (implements SimpleHasher trait)
        let tree = JellyfishMerkleTree::<_, sha2::Sha256>::new(self);

        // Get the root hash at this version
        // JMT will call get_node_option(), get_rightmost_leaf(), etc. via TreeReader
        match tree.get_root_hash(version) {
            Ok(root_hash) => {
                // Convert JMT RootHash ([u8; 32]) to B256
                Ok(B256::from_slice(&root_hash.0))
            }
            Err(e) => {
                // If root node not found (e.g., empty tree at version 0), return ZERO hash
                // This is expected for empty state before any commits
                if e.to_string().contains("Root node not found") {
                    Ok(B256::ZERO)
                } else {
                    Err(StateError::StorageError(format!(
                        "JMT get_root_hash failed: {}",
                        e
                    )))
                }
            }
        }
    }

    /// Get list of available snapshots
    pub fn list_snapshots(&self) -> Result<Vec<u64>, StateError> {
        let mut snapshots = Vec::new();
        let prefix = b"snapshot_";

        let iter = self.db.prefix_iterator(prefix);
        for item in iter {
            match item {
                Ok((key, _)) => {
                    if let Ok(key_str) = std::str::from_utf8(&key)
                        && let Some(version_str) = key_str.strip_prefix("snapshot_")
                        && let Ok(version) = version_str.parse::<u64>()
                    {
                        snapshots.push(version);
                    }
                }
                Err(e) => {
                    return Err(StateError::StorageError(format!(
                        "Failed to iterate snapshots: {}",
                        e
                    )));
                }
            }
        }

        snapshots.sort_unstable();
        Ok(snapshots)
    }

    /// Restore state from a snapshot
    pub fn restore_from_snapshot(&self, snapshot_version: u64) -> Result<(), StateError> {
        let snapshot_key = format!("snapshot_{}", snapshot_version);

        match self.db.get(snapshot_key.as_bytes()) {
            Ok(Some(_root_bytes)) => {
                *self.current_version.write().unwrap() = snapshot_version;
                self.db
                    .put(b"__current_version__", snapshot_version.to_le_bytes())
                    .map_err(|e| {
                        StateError::StorageError(format!("Failed to persist version: {}", e))
                    })?;
                log::info!(
                    "Restored state from snapshot at version {}",
                    snapshot_version
                );
                Ok(())
            }
            Ok(None) => Err(StateError::StorageError(format!(
                "Snapshot not found at version {}",
                snapshot_version
            ))),
            Err(e) => Err(StateError::StorageError(format!(
                "Failed to restore snapshot: {}",
                e
            ))),
        }
    }

    /// Get the current version from storage
    pub fn get_current_version(&self) -> u64 {
        *self.current_version.read().unwrap()
    }

    /// Get current database size estimate
    pub fn get_db_size(&self) -> Result<u64, StateError> {
        // RocksDB doesn't provide exact size easily, but we can get an estimate
        let mut total_size = 0u64;

        // This is approximate - in production, you'd use RocksDB properties
        let iter = self.db.iterator(rocksdb::IteratorMode::Start);
        for (_key, value) in iter.flatten() {
            total_size += value.len() as u64;
        }

        Ok(total_size)
    }

    /// Create a checkpoint (RocksDB native snapshot) at the specified path
    /// This is the recommended way to create consistent point-in-time snapshots.
    ///
    /// Benefits:
    /// - Consistent snapshot using hard links (fast, no data copy)
    /// - Minimal I/O overhead
    /// - Safe for concurrent writes
    /// - Can be used for backups, exports, or testing
    pub fn create_checkpoint<P: AsRef<Path>>(&self, checkpoint_path: P) -> Result<(), StateError> {
        use rocksdb::checkpoint::Checkpoint;

        let checkpoint = Checkpoint::new(&*self.db)
            .map_err(|e| StateError::StorageError(format!("Failed to create checkpoint: {}", e)))?;

        checkpoint
            .create_checkpoint(checkpoint_path)
            .map_err(|e| StateError::StorageError(format!("Failed to save checkpoint: {}", e)))?;

        Ok(())
    }

    /// Alternative: Create incremental backup using BackupEngine
    /// Best for production systems that need:
    /// - Incremental backups (save space)
    /// - Automatic retention policies
    /// - Backup verification
    /// - Restore to specific backup point
    #[cfg(feature = "backup-engine")]
    pub fn create_backup<P: AsRef<Path>>(&self, backup_path: P) -> Result<(), StateError> {
        use rocksdb::backup::{BackupEngine, BackupEngineOptions};

        let mut backup_opts = BackupEngineOptions::new(backup_path.as_ref()).map_err(|e| {
            StateError::StorageError(format!("Failed to create backup options: {}", e))
        })?;

        // Enable incremental backups (only changed files)
        backup_opts.set_share_table_files(true);

        let mut backup_engine = BackupEngine::open(&backup_opts, &*self.db).map_err(|e| {
            StateError::StorageError(format!("Failed to open backup engine: {}", e))
        })?;

        backup_engine
            .create_new_backup(&*self.db)
            .map_err(|e| StateError::StorageError(format!("Failed to create backup: {}", e)))?;

        // Optional: Keep only last N backups
        backup_engine
            .purge_old_backups(10)
            .map_err(|e| StateError::StorageError(format!("Failed to purge old backups: {}", e)))?;

        Ok(())
    }

    /// Restore from incremental backup
    #[cfg(feature = "backup-engine")]
    pub fn restore_from_backup<P: AsRef<Path>, Q: AsRef<Path>>(
        backup_path: P,
        restore_path: Q,
        backup_id: u32,
    ) -> Result<(), StateError> {
        use rocksdb::backup::{BackupEngine, BackupEngineOptions, RestoreOptions};

        let backup_opts = BackupEngineOptions::new(backup_path.as_ref()).map_err(|e| {
            StateError::StorageError(format!("Failed to create backup options: {}", e))
        })?;

        let backup_engine = BackupEngine::open(&backup_opts, &DB::open_default("dummy").unwrap())
            .map_err(|e| {
            StateError::StorageError(format!("Failed to open backup engine: {}", e))
        })?;

        let restore_opts = RestoreOptions::default();
        backup_engine
            .restore_from_backup(
                restore_path.as_ref(),
                restore_path.as_ref(),
                &restore_opts,
                backup_id,
            )
            .map_err(|e| {
                StateError::StorageError(format!("Failed to restore from backup: {}", e))
            })?;

        Ok(())
    }

    /// Get database statistics (useful for monitoring)
    pub fn get_statistics(&self) -> Option<String> {
        self.db.property_value("rocksdb.stats").ok().flatten()
    }

    /// Flush WAL and memtables to disk
    /// Useful before creating snapshots to ensure consistency
    pub fn flush(&self) -> Result<(), StateError> {
        self.db
            .flush()
            .map_err(|e| StateError::StorageError(format!("Failed to flush RocksDB: {}", e)))
    }
}

impl Drop for RocksDbStorage {
    fn drop(&mut self) {
        // Ensure data is flushed before closing
        if let Err(e) = self.db.flush() {
            log::error!("Failed to flush RocksDB on drop: {}", e);
        }
    }
}

// ============================================================================
// JMT Storage Trait Implementations
// ============================================================================

/// Implement TreeReader for JMT integration
///
/// This allows RocksDbStorage to be used as the storage backend for
/// JellyfishMerkleTree. The JMT will call these methods to read nodes.
impl TreeReader for RocksDbStorage {
    fn get_node_option(&self, node_key: &NodeKey) -> anyhow::Result<Option<Node>> {
        // Serialize the node key using borsh
        let key_bytes = borsh::to_vec(node_key)?;
        let db_key = format!("jmt_node_{}", hex::encode(&key_bytes));

        // Read from RocksDB
        match self.db.get(db_key.as_bytes())? {
            Some(value_bytes) => {
                // Deserialize the node using borsh
                let node: Node = borsh::from_slice(&value_bytes)?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    fn get_rightmost_leaf(&self) -> anyhow::Result<Option<(NodeKey, LeafNode)>> {
        // Query for the rightmost leaf (at current version)
        let version = *self.current_version.read().unwrap();
        let db_key = format!("jmt_rightmost_{}", version);

        match self.db.get(db_key.as_bytes())? {
            Some(value_bytes) => {
                // Deserialize using borsh
                let (node_key, leaf_node): (NodeKey, LeafNode) = borsh::from_slice(&value_bytes)?;
                Ok(Some((node_key, leaf_node)))
            }
            None => Ok(None),
        }
    }

    fn get_value_option(
        &self,
        max_version: Version,
        key_hash: KeyHash,
    ) -> anyhow::Result<Option<OwnedValue>> {
        // OPTIMIZATION: Use latest version tracking for O(1) lookup
        // Instead of iterating from max_version down to 0 (O(n)),
        // we lookup the latest version directly (O(1))

        let latest_version = self
            .get_latest_version(&key_hash)
            .map_err(|e| anyhow::anyhow!("Failed to get latest version: {}", e))?;

        if let Some(latest) = latest_version {
            // Fast path: We know the exact version to lookup
            if latest <= max_version {
                let key = format!("jmt_value_{}_{}", hex::encode(key_hash.0), latest);
                return Ok(self.db.get(key.as_bytes())?);
            }
            // If latest > max_version, fall through to slow scan
            // This happens during historical queries
        }

        // Fallback: Iterate backwards (only for historical queries or new keys)
        // This is the original O(n) algorithm, kept for correctness
        let key_prefix = format!("jmt_value_{}", hex::encode(key_hash.0));
        for v in (0..=max_version).rev() {
            let db_key = format!("{}_{}", key_prefix, v);
            if let Some(value_bytes) = self.db.get(db_key.as_bytes())? {
                return Ok(Some(value_bytes));
            }
        }

        Ok(None)
    }
}

/// Implement TreeWriter for JMT integration
///
/// This allows RocksDbStorage to write JMT nodes during state updates.
impl TreeWriter for RocksDbStorage {
    fn write_node_batch(&self, node_batch: &NodeBatch) -> anyhow::Result<()> {
        use rocksdb::WriteBatch;

        let mut batch = WriteBatch::default();

        // Write all nodes
        for (node_key, node) in node_batch.nodes() {
            let key_bytes = borsh::to_vec(node_key)?;
            let value_bytes = borsh::to_vec(node)?;
            let db_key = format!("jmt_node_{}", hex::encode(&key_bytes));
            batch.put(db_key.as_bytes(), &value_bytes);
        }

        // Note: stale node tracking would go here if needed for pruning
        // The jmt API may have changed and stale_node_index_batch() is not available
        // This is not critical for basic functionality

        // Execute batch write
        self.db.write(batch)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::StateKey;
    use alloy_primitives::Address;
    use tempfile::TempDir;

    #[test]
    fn test_rocksdb_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDbStorage::new(temp_dir.path(), PruningConfig::default()).unwrap();

        let key = StateKey::Balance {
            address: Address::ZERO,
            asset_id: 0,
        };

        // Set value
        storage.set(key.clone(), 1000u128).unwrap();

        // Get value
        let value: Option<u128> = storage.get(&key, 0).unwrap();
        assert_eq!(value, Some(1000));

        // Commit
        let root = storage.commit(1).unwrap();
        assert_ne!(root, B256::ZERO);
    }

    #[test]
    fn test_snapshots() {
        let temp_dir = TempDir::new().unwrap();
        let config = PruningConfig {
            snapshot_interval: 10,
            ..Default::default()
        };

        let storage = RocksDbStorage::new(temp_dir.path(), config).unwrap();

        // Create some versions with snapshots
        for version in 0..25 {
            storage.commit(version).unwrap();
        }

        // List snapshots
        let snapshots = storage.list_snapshots().unwrap();
        assert!(snapshots.contains(&0));
        assert!(snapshots.contains(&10));
        assert!(snapshots.contains(&20));
    }

    #[test]
    fn test_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let storage = RocksDbStorage::new(temp_dir.path(), PruningConfig::default()).unwrap();

        // Add some data
        let key = StateKey::Balance {
            address: Address::ZERO,
            asset_id: 0,
        };
        storage.set(key.clone(), 1000u128).unwrap();
        storage.commit(1).unwrap();

        // Create checkpoint
        let checkpoint_dir = temp_dir.path().join("checkpoint");
        storage.create_checkpoint(&checkpoint_dir).unwrap();

        // Verify checkpoint exists
        assert!(checkpoint_dir.exists());
    }
}
