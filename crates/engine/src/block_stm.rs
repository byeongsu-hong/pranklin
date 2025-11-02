use pranklin_state::{AccessMode, DeclareStateAccess, StateAccess};
use pranklin_tx::Transaction;
use std::collections::{HashMap, HashSet};

/// Transaction node in dependency graph
#[derive(Debug, Clone)]
pub struct TxNode {
    pub tx_index: usize,
    pub read_set: HashSet<StateAccess>,
    pub write_set: HashSet<StateAccess>,
}

/// Dependency graph for transaction scheduling
#[derive(Debug)]
pub struct DependencyGraph {
    nodes: Vec<TxNode>,
    // edges[i] = set of transaction indices that tx[i] depends on
    dependencies: Vec<HashSet<usize>>,
}

impl DependencyGraph {
    /// Build dependency graph from transactions
    pub fn build(txs: &[Transaction]) -> Self {
        let mut nodes = Vec::with_capacity(txs.len());
        let mut dependencies = vec![HashSet::new(); txs.len()];

        // Step 1: Collect read/write sets for each transaction
        for (idx, tx) in txs.iter().enumerate() {
            let accesses = tx.declare_accesses();
            
            let mut read_set = HashSet::new();
            let mut write_set = HashSet::new();
            
            for (access, mode) in accesses {
                match mode {
                    AccessMode::Read => {
                        read_set.insert(access);
                    }
                    AccessMode::Write => {
                        // Write implies read
                        read_set.insert(access);
                        write_set.insert(access);
                    }
                }
            }
            
            nodes.push(TxNode {
                tx_index: idx,
                read_set,
                write_set,
            });
        }

        // Step 2: Build dependency edges
        // Tx j depends on tx i if:
        // - i < j (i comes before j)
        // - write_set[i] ∩ read_set[j] ≠ ∅ (read-after-write)
        // - write_set[i] ∩ write_set[j] ≠ ∅ (write-after-write)
        for j in 0..nodes.len() {
            for i in 0..j {
                let conflict = 
                    // Read-after-write conflict
                    nodes[i].write_set.iter().any(|w| nodes[j].read_set.contains(w)) ||
                    // Write-after-write conflict
                    nodes[i].write_set.iter().any(|w| nodes[j].write_set.contains(w));
                
                if conflict {
                    dependencies[j].insert(i);
                }
            }
        }

        Self { nodes, dependencies }
    }

    /// Get groups of independent transactions that can execute in parallel
    pub fn independent_groups(&self) -> Vec<Vec<usize>> {
        let mut groups = Vec::new();
        let mut executed = HashSet::new();
        let mut remaining: HashSet<usize> = (0..self.nodes.len()).collect();

        while !remaining.is_empty() {
            // Find all transactions that can execute now
            let ready: Vec<usize> = remaining
                .iter()
                .filter(|&&idx| {
                    // All dependencies must be executed
                    self.dependencies[idx].iter().all(|dep| executed.contains(dep))
                })
                .copied()
                .collect();

            if ready.is_empty() {
                // Should not happen if graph is acyclic
                break;
            }

            // These transactions can execute in parallel
            groups.push(ready.clone());

            // Mark as executed
            for idx in ready {
                executed.insert(idx);
                remaining.remove(&idx);
            }
        }

        groups
    }

    /// Get transactions that have no dependencies (can start immediately)
    pub fn ready_transactions(&self) -> Vec<usize> {
        self.dependencies
            .iter()
            .enumerate()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Get degree of parallelism for this block
    pub fn parallelism_score(&self) -> f64 {
        let groups = self.independent_groups();
        let total_txs = self.nodes.len() as f64;
        let total_groups = groups.len() as f64;
        
        // Higher score = more parallelism
        // Score = average transactions per group
        if total_groups == 0.0 {
            0.0
        } else {
            total_txs / total_groups
        }
    }
}

/// Block-STM scheduler (future implementation)
/// 
/// This is a prototype/skeleton for parallel transaction execution.
/// Full implementation requires:
/// - Cloneable engine state
/// - Optimistic execution with conflict detection
/// - Re-execution strategy
/// - Integration with Rayon or Tokio
#[derive(Debug)]
pub struct BlockStmScheduler {
    /// Number of worker threads
    pub num_workers: usize,
    /// Whether to use optimistic execution
    pub optimistic: bool,
}

impl BlockStmScheduler {
    /// Create a new scheduler with specified worker count (0 = auto-detect)
    pub fn with_workers(num_workers: usize) -> Self {
        let num_workers = if num_workers == 0 {
            num_cpus::get()
        } else {
            num_workers
        };
        
        Self {
            num_workers,
            optimistic: false, // Conservative by default
        }
    }

    /// Analyze block for parallelism potential
    pub fn analyze_block(&self, txs: &[Transaction]) -> BlockAnalysis {
        let graph = DependencyGraph::build(txs);
        let groups = graph.independent_groups();
        
        BlockAnalysis {
            total_txs: txs.len(),
            num_groups: groups.len(),
            parallelism_score: graph.parallelism_score(),
            max_group_size: groups.iter().map(|g| g.len()).max().unwrap_or(0),
            ready_txs: graph.ready_transactions().len(),
        }
    }

    /// Decide whether to use parallel execution for this block
    pub fn should_parallelize(&self, analysis: &BlockAnalysis) -> bool {
        // Heuristics:
        // 1. At least 10 transactions
        // 2. Parallelism score > 1.5 (average 1.5 txs per group)
        // 3. At least 2 groups
        
        analysis.total_txs >= 10
            && analysis.parallelism_score >= 1.5
            && analysis.num_groups >= 2
    }
}

/// Analysis result for a block
#[derive(Debug, Clone)]
pub struct BlockAnalysis {
    pub total_txs: usize,
    pub num_groups: usize,
    pub parallelism_score: f64,
    pub max_group_size: usize,
    pub ready_txs: usize,
}

impl BlockAnalysis {
    pub fn is_parallel_friendly(&self) -> bool {
        self.parallelism_score >= 2.0
    }

    pub fn estimated_speedup(&self, num_workers: usize) -> f64 {
        if self.num_groups == 0 {
            return 1.0;
        }

        // Simple model: speedup = min(parallelism_score, num_workers)
        // In practice, it's more complex due to overhead
        let theoretical_speedup = self.parallelism_score.min(num_workers as f64);
        
        // Apply overhead factor (20% overhead for coordination)
        theoretical_speedup * 0.8
    }
}

/// Conflict detection for optimistic execution
#[derive(Debug, Default)]
pub struct ConflictDetector {
    // Track which transactions wrote to which state keys
    writes: HashMap<StateAccess, Vec<usize>>,
}

impl ConflictDetector {
    /// Record a write operation
    pub fn record_write(&mut self, tx_index: usize, access: StateAccess) {
        self.writes
            .entry(access)
            .or_insert_with(Vec::new)
            .push(tx_index);
    }

    /// Check if a transaction conflicts with earlier transactions
    pub fn has_conflict(&self, tx_index: usize, read_set: &HashSet<StateAccess>) -> bool {
        for access in read_set {
            if let Some(writers) = self.writes.get(access) {
                // Conflict if any earlier transaction wrote to this key
                if writers.iter().any(|&writer| writer < tx_index) {
                    return true;
                }
            }
        }
        false
    }

    /// Get all transactions that need re-execution due to conflicts
    pub fn get_conflicts(&self, nodes: &[TxNode]) -> Vec<usize> {
        let mut conflicts = Vec::new();
        
        for node in nodes {
            if self.has_conflict(node.tx_index, &node.read_set) {
                conflicts.push(node.tx_index);
            }
        }
        
        conflicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::Address;
    use pranklin_tx::{PlaceOrderTx, TransferTx, TxPayload, OrderType, TimeInForce};

    #[test]
    fn test_dependency_graph_independent_txs() {
        // Two transfers to different accounts = no dependency
        let tx1 = Transaction::new_raw(
            1,
            Address::from([1u8; 20]),
            TxPayload::Transfer(TransferTx {
                to: Address::from([2u8; 20]),
                asset_id: 0,
                amount: 100,
            }),
        );

        let tx2 = Transaction::new_raw(
            1,
            Address::from([3u8; 20]),
            TxPayload::Transfer(TransferTx {
                to: Address::from([4u8; 20]),
                asset_id: 0,
                amount: 200,
            }),
        );

        let graph = DependencyGraph::build(&[tx1, tx2]);
        let groups = graph.independent_groups();

        // Both should be in the same group (parallel)
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_dependency_graph_dependent_txs() {
        let addr = Address::from([1u8; 20]);

        // Two transfers from same account = sequential (nonce dependency)
        let tx1 = Transaction::new_raw(
            1,
            addr,
            TxPayload::Transfer(TransferTx {
                to: Address::from([2u8; 20]),
                asset_id: 0,
                amount: 100,
            }),
        );

        let tx2 = Transaction::new_raw(
            2,
            addr,
            TxPayload::Transfer(TransferTx {
                to: Address::from([3u8; 20]),
                asset_id: 0,
                amount: 200,
            }),
        );

        let graph = DependencyGraph::build(&[tx1, tx2]);
        let groups = graph.independent_groups();

        // Should be in different groups (sequential)
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 1);
        assert_eq!(groups[1].len(), 1);
    }

    #[test]
    fn test_dependency_graph_same_market() {
        let addr1 = Address::from([1u8; 20]);
        let addr2 = Address::from([2u8; 20]);

        // Two orders on same market = sequential (orderbook dependency)
        let tx1 = Transaction::new_raw(
            1,
            addr1,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50000,
                size: 10,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let tx2 = Transaction::new_raw(
            1,
            addr2,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: false,
                order_type: OrderType::Limit,
                price: 50000,
                size: 10,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let graph = DependencyGraph::build(&[tx1, tx2]);
        let groups = graph.independent_groups();

        // Should be sequential (both touch same OrderList)
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_dependency_graph_different_markets() {
        let addr1 = Address::from([1u8; 20]);
        let addr2 = Address::from([2u8; 20]);

        // Two orders on different markets = can be parallel
        let tx1 = Transaction::new_raw(
            1,
            addr1,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 0,
                is_buy: true,
                order_type: OrderType::Limit,
                price: 50000,
                size: 10,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let tx2 = Transaction::new_raw(
            1,
            addr2,
            TxPayload::PlaceOrder(PlaceOrderTx {
                market_id: 1, // Different market
                is_buy: false,
                order_type: OrderType::Limit,
                price: 3000,
                size: 20,
                time_in_force: TimeInForce::GTC,
                reduce_only: false,
                post_only: false,
            }),
        );

        let graph = DependencyGraph::build(&[tx1, tx2]);
        let groups = graph.independent_groups();

        // Should be parallel (different OrderLists)
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_scheduler_analysis() {
        let scheduler = BlockStmScheduler::with_workers(8);

        // Create a mix of independent and dependent transactions
        // Using separate address ranges for from and to to avoid balance conflicts
        let txs: Vec<Transaction> = (0..20)
            .map(|i| {
                Transaction::new_raw(
                    1,
                    Address::from([(i % 10) as u8; 20]),
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([(100 + (i % 10)) as u8; 20]), // Separate range
                        asset_id: 0,
                        amount: 100,
                    }),
                )
            })
            .collect();

        let analysis = scheduler.analyze_block(&txs);

        assert_eq!(analysis.total_txs, 20);
        // With same from addresses used twice, we expect some grouping
        // 10 unique addresses, each used twice = some parallelism but not full
        assert!(analysis.num_groups > 1);
        assert!(analysis.parallelism_score > 1.0);
    }

    #[test]
    fn test_conflict_detector() {
        let mut detector = ConflictDetector::default();

        let access = StateAccess::Balance {
            address: Address::from([1u8; 20]),
            asset_id: 0,
        };

        // Record writes
        detector.record_write(0, access.clone());
        detector.record_write(2, access.clone());

        // Check conflicts
        let mut read_set = HashSet::new();
        read_set.insert(access);

        // Tx 1 conflicts with tx 0
        assert!(detector.has_conflict(1, &read_set));

        // Tx 3 conflicts with both tx 0 and tx 2
        assert!(detector.has_conflict(3, &read_set));
    }

    #[test]
    fn test_parallelism_score() {
        // All independent transactions (to different recipients to avoid balance conflicts)
        let independent_txs: Vec<Transaction> = (0..10)
            .map(|i| {
                Transaction::new_raw(
                    1,
                    Address::from([i as u8; 20]),
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([(i + 100) as u8; 20]), // Non-overlapping recipients
                        asset_id: 0,
                        amount: 100,
                    }),
                )
            })
            .collect();

        let graph = DependencyGraph::build(&independent_txs);
        let score = graph.parallelism_score();

        // Should be 10.0 (all in one group, no conflicts)
        assert_eq!(score, 10.0);

        // All sequential transactions (same account)
        let addr = Address::from([1u8; 20]);
        let sequential_txs: Vec<Transaction> = (0..10)
            .map(|i| {
                Transaction::new_raw(
                    i + 1,
                    addr,
                    TxPayload::Transfer(TransferTx {
                        to: Address::from([2u8; 20]),
                        asset_id: 0,
                        amount: 100,
                    }),
                )
            })
            .collect();

        let graph = DependencyGraph::build(&sequential_txs);
        let score = graph.parallelism_score();

        // Should be 1.0 (one tx per group)
        assert_eq!(score, 1.0);
    }
}

