package grpc

import (
	"context"
	"crypto/tls"
	"fmt"
	"net"
	"net/http"
	"time"

	"connectrpc.com/connect"
	"golang.org/x/net/http2"
	"google.golang.org/protobuf/types/known/timestamppb"

	"github.com/evstack/ev-node/core/execution"
	pb "github.com/evstack/ev-node/types/pb/evnode/v1"
	"github.com/evstack/ev-node/types/pb/evnode/v1/v1connect"
)

// Ensure Client implements the execution.Executor interface
var _ execution.Executor = (*Client)(nil)

// Client is a Connect-RPC client that implements the execution.Executor interface.
// It communicates with the Pranklin execution service via Connect-RPC with HTTP/2 support.
type Client struct {
	client v1connect.ExecutorServiceClient
}

// NewClient creates a new Connect-RPC execution client for Pranklin with HTTP/2 support.
//
// Parameters:
// - url: The URL of the gRPC server (e.g., "http://localhost:50051")
//
// Returns:
// - *Client: The initialized Connect-RPC client with HTTP/2 transport
func NewClient(url string) *Client {
	// Create HTTP/2 client with h2c (HTTP/2 Cleartext) support
	httpClient := &http.Client{
		Transport: &http2.Transport{
			// Allow HTTP/2 without TLS
			AllowHTTP: true,
			// Custom dialer for h2c
			DialTLSContext: func(ctx context.Context, network, addr string, cfg *tls.Config) (net.Conn, error) {
				// Use plain TCP connection (no TLS)
				return net.Dial(network, addr)
			},
		},
	}

	return &Client{
		client: v1connect.NewExecutorServiceClient(
			httpClient,
			url,
		),
	}
}

// Close is a no-op for Connect-RPC clients (connection is managed by http.Client)
func (c *Client) Close() error {
	return nil
}

// InitChain initializes a new blockchain instance with genesis parameters.
func (c *Client) InitChain(ctx context.Context, genesisTime time.Time, initialHeight uint64, chainID string) (stateRoot []byte, maxBytes uint64, err error) {
	req := connect.NewRequest(&pb.InitChainRequest{
		GenesisTime:   timestamppb.New(genesisTime),
		InitialHeight: initialHeight,
		ChainId:       chainID,
	})

	resp, err := c.client.InitChain(ctx, req)
	if err != nil {
		return nil, 0, fmt.Errorf("connect client: failed to init chain: %w", err)
	}

	return resp.Msg.StateRoot, resp.Msg.MaxBytes, nil
}

// GetTxs fetches available transactions from the execution layer's mempool.
func (c *Client) GetTxs(ctx context.Context) ([][]byte, error) {
	req := connect.NewRequest(&pb.GetTxsRequest{})

	resp, err := c.client.GetTxs(ctx, req)
	if err != nil {
		return nil, fmt.Errorf("connect client: failed to get txs: %w", err)
	}

	return resp.Msg.Txs, nil
}

// ExecuteTxs processes transactions to produce a new block state.
func (c *Client) ExecuteTxs(ctx context.Context, txs [][]byte, blockHeight uint64, timestamp time.Time, prevStateRoot []byte) (updatedStateRoot []byte, maxBytes uint64, err error) {
	req := connect.NewRequest(&pb.ExecuteTxsRequest{
		Txs:           txs,
		BlockHeight:   blockHeight,
		Timestamp:     timestamppb.New(timestamp),
		PrevStateRoot: prevStateRoot,
	})

	resp, err := c.client.ExecuteTxs(ctx, req)
	if err != nil {
		return nil, 0, fmt.Errorf("connect client: failed to execute txs: %w", err)
	}

	return resp.Msg.UpdatedStateRoot, resp.Msg.MaxBytes, nil
}

// SetFinal marks a block as finalized at the specified height.
func (c *Client) SetFinal(ctx context.Context, blockHeight uint64) error {
	req := connect.NewRequest(&pb.SetFinalRequest{
		BlockHeight: blockHeight,
	})

	_, err := c.client.SetFinal(ctx, req)
	if err != nil {
		return fmt.Errorf("connect client: failed to set final: %w", err)
	}

	return nil
}
