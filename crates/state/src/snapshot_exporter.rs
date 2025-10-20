use crate::{RocksDbStorage, StateError};
use std::fs::File;
use std::path::{Path, PathBuf};
use tar::Builder as TarBuilder;

/// Snapshot metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotMetadata {
    /// Block height
    pub height: u64,
    /// State root hash
    pub state_root: String,
    /// Timestamp
    pub timestamp: u64,
    /// Database size in bytes
    pub db_size: u64,
    /// Compressed snapshot size
    pub snapshot_size: u64,
    /// Chain ID
    pub chain_id: String,
    /// Version
    pub version: String,
}

/// Cloud storage provider configuration
#[derive(Debug, Clone)]
pub enum CloudProvider {
    /// AWS S3
    S3(S3Config),
    /// Google Cloud Storage
    GCS(GCSConfig),
    /// Local filesystem
    Local { path: PathBuf },
}

/// AWS S3 configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    pub prefix: String,
}

impl S3Config {
    pub fn new(
        bucket: impl Into<String>,
        region: impl Into<String>,
        prefix: impl Into<String>,
    ) -> Self {
        Self {
            bucket: bucket.into(),
            region: region.into(),
            prefix: prefix.into(),
        }
    }
}

/// Google Cloud Storage configuration
#[derive(Debug, Clone)]
pub struct GCSConfig {
    pub bucket: String,
    pub prefix: String,
}

impl GCSConfig {
    pub fn new(bucket: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            prefix: prefix.into(),
        }
    }
}

/// Snapshot exporter configuration
#[derive(Debug, Clone)]
pub struct SnapshotExporterConfig {
    /// Cloud provider settings
    pub provider: CloudProvider,
    /// Auto-export interval in blocks (0 = disabled)
    pub auto_export_interval: u64,
    /// Chain ID
    pub chain_id: String,
}

/// Snapshot exporter for RocksDB state
/// Always uses LZ4 compression for fast validator bootstrapping
#[derive(Clone)]
pub struct SnapshotExporter {
    config: SnapshotExporterConfig,
    last_export_height: u64,
}

impl SnapshotExporter {
    /// Create a new snapshot exporter
    pub fn new(config: SnapshotExporterConfig) -> Self {
        Self {
            config,
            last_export_height: 0,
        }
    }

    /// Check if we should export a snapshot at this height
    pub fn should_export(&self, height: u64) -> bool {
        if self.config.auto_export_interval == 0 {
            return false;
        }

        height > 0 && height >= self.last_export_height + self.config.auto_export_interval
    }

    /// Export snapshot to configured destination
    ///
    /// This uses RocksDB's native Checkpoint feature for consistent snapshots:
    /// 1. Flushes WAL and memtables to ensure all data is on disk
    /// 2. Creates checkpoint using hard links (fast, no data copy)
    /// 3. Compresses with LZ4 for fast validator bootstrapping
    /// 4. Uploads to cloud storage
    pub async fn export_snapshot(
        &mut self,
        storage: &RocksDbStorage,
        db_path: &Path,
        height: u64,
    ) -> Result<SnapshotMetadata, StateError> {
        log::info!("Starting snapshot export at height {}", height);

        // Flush all data to disk before creating checkpoint
        // This ensures the checkpoint contains all committed data
        storage.flush()?;

        // Create temporary directory for snapshot creation
        let temp_dir = std::env::temp_dir().join(format!("pranklin_snapshot_{}", height));
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| StateError::StorageError(format!("Failed to create temp dir: {}", e)))?;

        // Create checkpoint using RocksDB's native checkpoint API
        // This creates a consistent point-in-time snapshot using hard links
        // - Fast: No data copying, just hard links to SST files
        // - Consistent: Guarantees transactional consistency
        // - Safe: Works even with concurrent writes to main DB
        let checkpoint_path = temp_dir.join("checkpoint");
        storage
            .create_checkpoint(&checkpoint_path)
            .map_err(|e| StateError::StorageError(format!("Failed to create checkpoint: {}", e)))?;

        // Get metadata
        let state_root = storage.get_root(height);
        let db_size = get_dir_size(db_path)?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Compress with LZ4
        let snapshot_path = temp_dir.join(format!("pranklin-snapshot-{}.tar.lz4", height));
        self.compress_lz4(&checkpoint_path, &snapshot_path)?;

        let snapshot_size = std::fs::metadata(&snapshot_path)
            .map_err(|e| StateError::StorageError(format!("Failed to get snapshot size: {}", e)))?
            .len();

        // Create metadata
        let metadata = SnapshotMetadata {
            height,
            state_root: format!("0x{}", hex::encode(state_root.as_slice())),
            timestamp,
            db_size,
            snapshot_size,
            chain_id: self.config.chain_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Save metadata
        let metadata_path = temp_dir.join(format!("pranklin-snapshot-{}.json", height));
        let metadata_json = serde_json::to_string_pretty(&metadata).map_err(|e| {
            StateError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;
        std::fs::write(&metadata_path, metadata_json)
            .map_err(|e| StateError::StorageError(format!("Failed to write metadata: {}", e)))?;

        // Upload to destination
        match &self.config.provider {
            CloudProvider::S3(config) => {
                self.upload_to_s3(&snapshot_path, &metadata_path, config, height)
                    .await?;
            }
            CloudProvider::GCS(config) => {
                self.upload_to_gcs(&snapshot_path, &metadata_path, config, height)
                    .await?;
            }
            CloudProvider::Local { path } => {
                self.copy_to_local(&snapshot_path, &metadata_path, path, height)?;
            }
        }

        // Cleanup temp directory
        std::fs::remove_dir_all(&temp_dir)
            .map_err(|e| StateError::StorageError(format!("Failed to cleanup temp dir: {}", e)))?;

        // Update last export height
        self.last_export_height = height;

        log::info!(
            "Snapshot export completed: {:.2} MB",
            snapshot_size as f64 / 1_000_000.0
        );

        Ok(metadata)
    }

    /// Compress directory with LZ4
    fn compress_lz4(&self, source_dir: &Path, output_path: &Path) -> Result<(), StateError> {
        let tar_gz = File::create(output_path)
            .map_err(|e| StateError::StorageError(format!("Failed to create archive: {}", e)))?;

        let encoder = lz4::EncoderBuilder::new()
            .level(4) // Fast compression for validators
            .build(tar_gz)
            .map_err(|e| {
                StateError::StorageError(format!("Failed to create LZ4 encoder: {}", e))
            })?;

        let mut tar = TarBuilder::new(encoder);
        tar.append_dir_all(".", source_dir)
            .map_err(|e| StateError::StorageError(format!("Failed to create tar: {}", e)))?;

        let encoder = tar
            .into_inner()
            .map_err(|e| StateError::StorageError(format!("Failed to finalize tar: {}", e)))?;

        let (_writer, result) = encoder.finish();
        result.map_err(|e| StateError::StorageError(format!("Failed to finalize LZ4: {}", e)))?;

        Ok(())
    }

    /// Upload to AWS S3
    async fn upload_to_s3(
        &self,
        snapshot_path: &Path,
        metadata_path: &Path,
        s3_config: &S3Config,
        height: u64,
    ) -> Result<(), StateError> {
        use aws_config::BehaviorVersion;
        use aws_sdk_s3::primitives::ByteStream;

        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(s3_config.region.clone()))
            .load()
            .await;

        let client = aws_sdk_s3::Client::new(&config);

        // Upload snapshot
        let snapshot_key = format!("{}/pranklin-snapshot-{}.tar.lz4", s3_config.prefix, height);
        let snapshot_data = std::fs::read(snapshot_path)
            .map_err(|e| StateError::StorageError(format!("Failed to read snapshot: {}", e)))?;

        client
            .put_object()
            .bucket(&s3_config.bucket)
            .key(&snapshot_key)
            .body(ByteStream::from(snapshot_data))
            .content_type("application/x-lz4")
            .send()
            .await
            .map_err(|e| {
                StateError::StorageError(format!("Failed to upload snapshot to S3: {}", e))
            })?;

        log::info!(
            "Uploaded snapshot to s3://{}/{}",
            s3_config.bucket,
            snapshot_key
        );

        // Upload metadata
        let metadata_key = format!("{}/pranklin-snapshot-{}.json", s3_config.prefix, height);
        let metadata_data = std::fs::read(metadata_path)
            .map_err(|e| StateError::StorageError(format!("Failed to read metadata: {}", e)))?;

        client
            .put_object()
            .bucket(&s3_config.bucket)
            .key(&metadata_key)
            .body(ByteStream::from(metadata_data))
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| {
                StateError::StorageError(format!("Failed to upload metadata to S3: {}", e))
            })?;

        // Update latest.json
        let latest_content = serde_json::json!({
            "height": height,
            "snapshot_url": format!("s3://{}/{}", s3_config.bucket, snapshot_key),
            "metadata_url": format!("s3://{}/{}", s3_config.bucket, metadata_key),
        });

        let latest_key = format!("{}/latest.json", s3_config.prefix);
        client
            .put_object()
            .bucket(&s3_config.bucket)
            .key(&latest_key)
            .body(ByteStream::from(
                serde_json::to_vec(&latest_content).unwrap(),
            ))
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| {
                StateError::StorageError(format!("Failed to upload latest.json to S3: {}", e))
            })?;

        log::info!("Updated latest snapshot pointer");

        Ok(())
    }

    /// Upload to Google Cloud Storage
    ///
    /// Uses the official google-cloud-storage crate (v1.1.0+) which provides
    /// a production-ready client. Requires GOOGLE_APPLICATION_CREDENTIALS
    /// environment variable pointing to a service account JSON key.
    ///
    /// Reference: https://docs.rs/google-cloud-storage/latest
    async fn upload_to_gcs(
        &self,
        snapshot_path: &Path,
        metadata_path: &Path,
        gcs_config: &GCSConfig,
        height: u64,
    ) -> Result<(), StateError> {
        use google_cloud_storage::client::Storage;

        log::info!(
            "Uploading snapshot to GCS: gs://{}/{}",
            gcs_config.bucket,
            gcs_config.prefix
        );

        // Initialize GCS Storage client with default credentials
        // This will use GOOGLE_APPLICATION_CREDENTIALS environment variable
        let storage = Storage::builder().build().await.map_err(|e| {
            StateError::StorageError(format!(
                "Failed to initialize GCS client. \
                 Ensure GOOGLE_APPLICATION_CREDENTIALS environment variable is set \
                 and points to a valid service account JSON key: {}",
                e
            ))
        })?;

        // Upload snapshot file
        let snapshot_name = format!("{}/pranklin-snapshot-{}.tar.lz4", gcs_config.prefix, height);
        log::info!("Uploading snapshot file: {}", snapshot_name);

        let snapshot_data = tokio::fs::read(snapshot_path).await.map_err(|e| {
            StateError::StorageError(format!("Failed to read snapshot file: {}", e))
        })?;

        let bucket_name = format!("projects/_/buckets/{}", gcs_config.bucket);
        storage
            .write_object(
                &bucket_name,
                &snapshot_name,
                bytes::Bytes::from(snapshot_data),
            )
            .send_buffered()
            .await
            .map_err(|e| {
                StateError::StorageError(format!(
                    "Failed to upload snapshot to gs://{}/{}. \
                     Ensure bucket exists and you have write permissions: {}",
                    gcs_config.bucket, snapshot_name, e
                ))
            })?;

        log::info!(
            "✓ Snapshot uploaded: gs://{}/{}",
            gcs_config.bucket,
            snapshot_name
        );

        // Upload metadata file
        let metadata_name = format!("{}/pranklin-snapshot-{}.json", gcs_config.prefix, height);
        log::info!("Uploading metadata file: {}", metadata_name);

        let metadata_data = tokio::fs::read(metadata_path).await.map_err(|e| {
            StateError::StorageError(format!("Failed to read metadata file: {}", e))
        })?;

        storage
            .write_object(
                &bucket_name,
                &metadata_name,
                bytes::Bytes::from(metadata_data),
            )
            .send_buffered()
            .await
            .map_err(|e| {
                StateError::StorageError(format!(
                    "Failed to upload metadata to gs://{}/{}: {}",
                    gcs_config.bucket, metadata_name, e
                ))
            })?;

        log::info!(
            "✓ Metadata uploaded: gs://{}/{}",
            gcs_config.bucket,
            metadata_name
        );

        // Create/update latest.json pointer
        let latest_content = serde_json::json!({
            "height": height,
            "snapshot_url": format!("gs://{}/{}", gcs_config.bucket, snapshot_name),
            "metadata_url": format!("gs://{}/{}", gcs_config.bucket, metadata_name),
            "updated_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        });

        let latest_name = format!("{}/latest.json", gcs_config.prefix);
        let latest_data = serde_json::to_vec_pretty(&latest_content).map_err(|e| {
            StateError::StorageError(format!("Failed to serialize latest.json: {}", e))
        })?;

        storage
            .write_object(&bucket_name, &latest_name, bytes::Bytes::from(latest_data))
            .send_buffered()
            .await
            .map_err(|e| {
                StateError::StorageError(format!(
                    "Failed to upload latest.json to gs://{}/{}: {}",
                    gcs_config.bucket, latest_name, e
                ))
            })?;

        log::info!(
            "✓ Latest pointer updated: gs://{}/{}",
            gcs_config.bucket,
            latest_name
        );
        log::info!(
            "✓ GCS upload complete: 3 files uploaded to gs://{}/{}",
            gcs_config.bucket,
            gcs_config.prefix
        );

        Ok(())
    }

    /// Copy to local filesystem
    fn copy_to_local(
        &self,
        snapshot_path: &Path,
        metadata_path: &Path,
        dest_dir: &Path,
        height: u64,
    ) -> Result<(), StateError> {
        std::fs::create_dir_all(dest_dir).map_err(|e| {
            StateError::StorageError(format!("Failed to create destination directory: {}", e))
        })?;

        // Copy snapshot
        let dest_snapshot = dest_dir.join(format!("pranklin-snapshot-{}.tar.lz4", height));
        std::fs::copy(snapshot_path, &dest_snapshot)
            .map_err(|e| StateError::StorageError(format!("Failed to copy snapshot: {}", e)))?;

        // Copy metadata
        let dest_metadata = dest_dir.join(format!("pranklin-snapshot-{}.json", height));
        std::fs::copy(metadata_path, &dest_metadata)
            .map_err(|e| StateError::StorageError(format!("Failed to copy metadata: {}", e)))?;

        // Create latest.json
        let latest_content = serde_json::json!({
            "height": height,
            "snapshot_file": dest_snapshot.to_string_lossy(),
            "metadata_file": dest_metadata.to_string_lossy(),
        });

        let latest_path = dest_dir.join("latest.json");
        std::fs::write(
            &latest_path,
            serde_json::to_string_pretty(&latest_content).unwrap(),
        )
        .map_err(|e| StateError::StorageError(format!("Failed to write latest.json: {}", e)))?;

        log::info!("Copied snapshot to {:?}", dest_dir);

        Ok(())
    }
}

/// Get directory size recursively
fn get_dir_size(path: &Path) -> Result<u64, StateError> {
    let mut size = 0;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)
            .map_err(|e| StateError::StorageError(format!("Failed to read directory: {}", e)))?
        {
            let entry = entry
                .map_err(|e| StateError::StorageError(format!("Failed to read entry: {}", e)))?;
            let path = entry.path();
            if path.is_dir() {
                size += get_dir_size(&path)?;
            } else {
                size += entry
                    .metadata()
                    .map_err(|e| {
                        StateError::StorageError(format!("Failed to get metadata: {}", e))
                    })?
                    .len();
            }
        }
    }
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_metadata_serialization() {
        let metadata = SnapshotMetadata {
            height: 1000,
            state_root: "0x1234".to_string(),
            timestamp: 1234567890,
            db_size: 1000000,
            snapshot_size: 500000,
            chain_id: "pranklin-mainnet-1".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: SnapshotMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata.height, deserialized.height);
        assert_eq!(metadata.state_root, deserialized.state_root);
    }
}
