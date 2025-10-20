package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	evcmd "github.com/evstack/ev-node/pkg/cmd"
	"github.com/evstack/ev-node/pkg/config"
)

func main() {
	// Initiate the root command
	rootCmd := &cobra.Command{
		Use:   "pranklin-sequencer",
		Short: "Pranklin Sequencer - Perpetual DEX with EV-Node consensus",
		Long: `Run a Pranklin sequencer node with EV-Node consensus framework.
Connects to Pranklin execution layer via gRPC for trading operations.`,
	}

	config.AddGlobalFlags(rootCmd, "pranklin-sequencer")

	rootCmd.AddCommand(
		InitCmd(),
		NodeCmd, // Unified node command (DA + Execution + Sequencer)
		RunCmd,  // Legacy: sequencer only (requires external DA + Execution)
		evcmd.VersionCmd,
		evcmd.NetInfoCmd,
		evcmd.StoreUnsafeCleanCmd,
		evcmd.KeysCmd(),
	)

	if err := rootCmd.Execute(); err != nil {
		// Print to stderr and exit with error
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
