package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"sync"
	"syscall"
	"time"

	"github.com/spf13/cobra"

	"github.com/evstack/ev-node/core/da"
	"github.com/evstack/ev-node/da/jsonrpc"
	"github.com/evstack/ev-node/node"
	rollcmd "github.com/evstack/ev-node/pkg/cmd"
	"github.com/evstack/ev-node/pkg/config"
	rollgenesis "github.com/evstack/ev-node/pkg/genesis"
	"github.com/evstack/ev-node/pkg/p2p"
	"github.com/evstack/ev-node/pkg/p2p/key"
	"github.com/evstack/ev-node/pkg/store"
	"github.com/evstack/ev-node/sequencers/single"

	"github.com/pranklin/pranklin-sequencer/grpc"
)

const (
	// FlagLocalDABinary is the flag for the local-da binary path
	FlagLocalDABinary = "local-da-binary"
	// FlagLocalDAPort is the flag for the local-da port
	FlagLocalDAPort = "local-da-port"
	// FlagExecutionBinary is the flag for the execution binary path
	FlagExecutionBinary = "execution-binary"
	// FlagExecutionGrpcAddr is the flag for the execution gRPC address
	FlagExecutionGrpcAddr = "execution-grpc-addr"
	// FlagExecutionRpcAddr is the flag for the execution RPC address
	FlagExecutionRpcAddr = "execution-rpc-addr"
	// FlagExecutionDBPath is the flag for the execution database path
	FlagExecutionDBPath = "execution-db-path"
	// FlagBridgeOperators is the flag for bridge operator addresses
	FlagBridgeOperators = "bridge-operators"
)

var NodeCmd = &cobra.Command{
	Use:     "node",
	Aliases: []string{"unified", "all"},
	Short:   "Run a unified Pranklin node (DA + Execution + Sequencer)",
	Long: `Start a unified Pranklin node that manages all components:
  - Local DA layer for data availability
  - Execution layer for trading operations
  - Sequencer for consensus and block production

This is similar to how Cosmos nodes embed Tendermint.
All components run as managed subprocesses with graceful shutdown.`,
	RunE: func(cmd *cobra.Command, args []string) error {
		ctx, cancel := context.WithCancel(cmd.Context())
		defer cancel()

		// Parse flags
		localDABinary, _ := cmd.Flags().GetString(FlagLocalDABinary)
		localDAPort, _ := cmd.Flags().GetString(FlagLocalDAPort)
		executionBinary, _ := cmd.Flags().GetString(FlagExecutionBinary)
		executionGrpcAddr, _ := cmd.Flags().GetString(FlagExecutionGrpcAddr)
		executionRpcAddr, _ := cmd.Flags().GetString(FlagExecutionRpcAddr)
		executionDBPath, _ := cmd.Flags().GetString(FlagExecutionDBPath)
		bridgeOperators, _ := cmd.Flags().GetString(FlagBridgeOperators)
		chainID, _ := cmd.Flags().GetString(rollgenesis.ChainIDFlag)

		// Parse node configuration
		nodeConfig, err := rollcmd.ParseConfig(cmd)
		if err != nil {
			return err
		}

		logger := rollcmd.SetupLogger(nodeConfig.Log)

		// Validate binary paths
		if _, err := exec.LookPath(localDABinary); err != nil {
			return fmt.Errorf("local-da binary not found: %s\nPlease install it or specify the correct path with --local-da-binary", localDABinary)
		}

		// Check if execution binary exists (could be absolute or relative path)
		if _, err := os.Stat(executionBinary); err != nil {
			// Try to find it in PATH
			if _, pathErr := exec.LookPath(executionBinary); pathErr != nil {
				return fmt.Errorf("execution binary not found: %s\nPlease build it first: cd .. && cargo build --release --bin pranklin-app\nOr specify the correct path with --execution-binary", executionBinary)
			}
		}

		logger.Info().Msg("🚀 Starting Pranklin Unified Node")
		logger.Info().Msg("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")

		// Setup signal handling for graceful shutdown
		sigChan := make(chan os.Signal, 1)
		signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)

		// Track all subprocesses
		var wg sync.WaitGroup
		var mu sync.Mutex
		processes := make([]*exec.Cmd, 0)
		errChan := make(chan error, 3)

		// Cleanup function
		cleanup := func() {
			logger.Info().Msg("🛑 Shutting down all components...")
			mu.Lock()
			defer mu.Unlock()

			for i := len(processes) - 1; i >= 0; i-- {
				proc := processes[i]
				if proc != nil && proc.Process != nil {
					logger.Info().Int("pid", proc.Process.Pid).Msg("Stopping process")
					_ = proc.Process.Signal(syscall.SIGTERM)

					// Wait for graceful shutdown with timeout
					done := make(chan error, 1)
					go func() {
						done <- proc.Wait()
					}()

					select {
					case <-done:
						logger.Info().Int("pid", proc.Process.Pid).Msg("Process stopped gracefully")
					case <-time.After(5 * time.Second):
						logger.Warn().Int("pid", proc.Process.Pid).Msg("Force killing process")
						_ = proc.Process.Kill()
					}
				}
			}
		}

		// Start Local DA
		logger.Info().Str("binary", localDABinary).Str("port", localDAPort).Msg("📦 Starting Local DA layer...")
		daCmd := exec.CommandContext(ctx, localDABinary, "-port", localDAPort)
		daCmd.Stdout = os.Stdout
		daCmd.Stderr = os.Stderr

		if err := daCmd.Start(); err != nil {
			return fmt.Errorf("failed to start Local DA: %w", err)
		}

		mu.Lock()
		processes = append(processes, daCmd)
		mu.Unlock()

		logger.Info().Int("pid", daCmd.Process.Pid).Msg("✅ Local DA started")

		wg.Add(1)
		go func() {
			defer wg.Done()
			if err := daCmd.Wait(); err != nil {
				logger.Error().Err(err).Msg("Local DA exited with error")
				errChan <- fmt.Errorf("Local DA failed: %w", err)
			}
		}()

		// Wait for DA to be ready
		time.Sleep(2 * time.Second)

		// Start Execution layer
		logger.Info().
			Str("binary", executionBinary).
			Str("grpc", executionGrpcAddr).
			Str("rpc", executionRpcAddr).
			Msg("⚙️  Starting Execution layer...")

		execArgs := []string{
			"start",
			"--grpc.addr", executionGrpcAddr,
			"--rpc.addr", executionRpcAddr,
			"--db.path", executionDBPath,
			"--chain.id", chainID,
		}

		if bridgeOperators != "" {
			execArgs = append(execArgs, "--bridge.operators", bridgeOperators)
		}

		execCmd := exec.CommandContext(ctx, executionBinary, execArgs...)
		execCmd.Stdout = os.Stdout
		execCmd.Stderr = os.Stderr

		if err := execCmd.Start(); err != nil {
			cleanup()
			return fmt.Errorf("failed to start Execution layer: %w", err)
		}

		mu.Lock()
		processes = append(processes, execCmd)
		mu.Unlock()

		logger.Info().Int("pid", execCmd.Process.Pid).Msg("✅ Execution layer started")

		wg.Add(1)
		go func() {
			defer wg.Done()
			if err := execCmd.Wait(); err != nil {
				logger.Error().Err(err).Msg("Execution layer exited with error")
				errChan <- fmt.Errorf("Execution layer failed: %w", err)
			}
		}()

		// Wait for Execution to be ready
		time.Sleep(3 * time.Second)

		// Create gRPC execution client
		logger.Info().Msg("🔗 Connecting to Execution layer...")
		executor := grpc.NewClient("http://" + executionGrpcAddr)

		// Setup DA client
		daAddress := fmt.Sprintf("http://127.0.0.1:%s", localDAPort)
		logger.Info().Str("address", daAddress).Msg("🔗 Connecting to Local DA...")

		headerNamespace := da.NamespaceFromString(nodeConfig.DA.GetNamespace())
		dataNamespace := da.NamespaceFromString(nodeConfig.DA.GetDataNamespace())

		daJrpc, err := jsonrpc.NewClient(ctx, logger, daAddress, "", nodeConfig.DA.GasPrice, nodeConfig.DA.GasMultiplier, rollcmd.DefaultMaxBlobSize)
		if err != nil {
			cleanup()
			return fmt.Errorf("failed to create DA client: %w", err)
		}

		// Create datastore
		datastore, err := store.NewDefaultKVStore(nodeConfig.RootDir, nodeConfig.DBPath, "pranklin-sequencer")
		if err != nil {
			cleanup()
			return err
		}

		// Load genesis
		genesis, err := rollgenesis.LoadGenesis(rollgenesis.GenesisPath(nodeConfig.RootDir))
		if err != nil {
			cleanup()
			return err
		}

		if genesis.DAStartHeight == 0 && !nodeConfig.Node.Aggregator {
			logger.Warn().Msg("da_start_height is not set in genesis.json")
		}

		// Create metrics provider
		singleMetrics, err := single.DefaultMetricsProvider(nodeConfig.Instrumentation.IsPrometheusEnabled())(genesis.ChainID)
		if err != nil {
			cleanup()
			return err
		}

		// Create sequencer
		logger.Info().Msg("🎯 Starting Sequencer...")
		sequencer, err := single.NewSequencer(
			ctx,
			logger,
			datastore,
			&daJrpc.DA,
			[]byte(genesis.ChainID),
			nodeConfig.Node.BlockTime.Duration,
			singleMetrics,
			nodeConfig.Node.Aggregator,
		)
		if err != nil {
			cleanup()
			return err
		}

		// Load node key
		nodeKey, err := key.LoadNodeKey(filepath.Dir(nodeConfig.ConfigPath()))
		if err != nil {
			cleanup()
			return err
		}

		// Create P2P client
		p2pClient, err := p2p.NewClient(nodeConfig.P2P, nodeKey.PrivKey, datastore, genesis.ChainID, logger, nil)
		if err != nil {
			cleanup()
			return err
		}

		logger.Info().Msg("✅ Sequencer initialized")
		logger.Info().Msg("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
		logger.Info().Str("DA", daAddress).Str("Execution gRPC", executionGrpcAddr).Str("Execution RPC", executionRpcAddr).Msg("📡 Component addresses")
		logger.Info().Str("Header NS", headerNamespace.HexString()).Str("Data NS", dataNamespace.HexString()).Msg("📋 Namespaces")
		logger.Info().Msg("🎉 Pranklin Unified Node is running!")
		logger.Info().Msg("Press Ctrl+C to stop")
		logger.Info().Msg("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")

		// Start the node in a goroutine
		wg.Add(1)
		go func() {
			defer wg.Done()
			if err := rollcmd.StartNode(logger, cmd, executor, sequencer, &daJrpc.DA, p2pClient, datastore, nodeConfig, genesis, node.NodeOptions{}); err != nil {
				logger.Error().Err(err).Msg("Sequencer failed")
				errChan <- fmt.Errorf("Sequencer failed: %w", err)
			}
		}()

		// Wait for shutdown signal or error
		select {
		case sig := <-sigChan:
			logger.Info().Str("signal", sig.String()).Msg("Received shutdown signal")
			cancel()
			cleanup()
		case err := <-errChan:
			logger.Error().Err(err).Msg("Component failed, shutting down")
			cancel()
			cleanup()
			return err
		}

		// Wait for all goroutines to finish
		wg.Wait()
		logger.Info().Msg("✅ Pranklin Unified Node stopped")

		return nil
	},
}

func init() {
	// Add configuration flags
	config.AddFlags(NodeCmd)

	// Add unified node specific flags
	NodeCmd.Flags().String(FlagLocalDABinary, "local-da", "Path to local-da binary")
	NodeCmd.Flags().String(FlagLocalDAPort, "7980", "Port for local-da")
	NodeCmd.Flags().String(FlagExecutionBinary, "../target/release/pranklin-app", "Path to pranklin-app execution binary")
	NodeCmd.Flags().String(FlagExecutionGrpcAddr, "0.0.0.0:50051", "Execution layer gRPC address")
	NodeCmd.Flags().String(FlagExecutionRpcAddr, "0.0.0.0:3000", "Execution layer RPC address")
	NodeCmd.Flags().String(FlagExecutionDBPath, "./data/pranklin_db", "Execution layer database path")
	NodeCmd.Flags().String(FlagBridgeOperators, "", "Bridge operator addresses (comma-separated)")
	NodeCmd.Flags().String(rollgenesis.ChainIDFlag, "pranklin-mainnet-1", "Chain ID for execution layer")
}
