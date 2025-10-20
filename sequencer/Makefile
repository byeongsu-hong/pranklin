.PHONY: help build install run clean deps test proto

help: ## Display this help message
	@echo "Pranklin Sequencer - Makefile commands:"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

deps: ## Download Go dependencies
	@echo "📦 Downloading dependencies..."
	go mod download
	go mod tidy
	@echo "✅ Dependencies ready"


build: ## Build the pranklin-sequencer binary
	@echo "🔨 Building pranklin-sequencer..."
	go build -o bin/pranklin-sequencer ./cmd
	@echo "✅ Binary built: bin/pranklin-sequencer"

install: ## Install pranklin-sequencer to GOPATH/bin
	@echo "📦 Installing pranklin-sequencer..."
	go install ./cmd
	@echo "✅ Installed to $(shell go env GOPATH)/bin/pranklin-sequencer"

build-all: ## Build both execution layer and sequencer
	@echo "🔨 Building Pranklin components..."
	@cd .. && cargo build --release --bin pranklin-app
	@$(MAKE) build
	@echo "✅ All components built"

run-node: build ## Run unified node (DA + Execution + Sequencer)
	@echo "🚀 Starting Pranklin Unified Node..."
	./bin/pranklin-sequencer node \
		--root-dir ~/.pranklin-sequencer \
		--local-da-binary local-da \
		--execution-binary ../target/release/pranklin-app

run: build ## Build and run the sequencer (requires execution and DA layer)
	@echo "🚀 Starting Pranklin Sequencer..."
	./bin/pranklin-sequencer start

run-dev: build ## Run in development mode with debug output
	@echo "🚀 Starting Pranklin Sequencer (dev mode)..."
	./bin/pranklin-sequencer start \
		--grpc-executor-url localhost:50051 \
		--da.address http://localhost:7980 \
		--root-dir ~/.pranklin-sequencer \
		--chain-id pranklin-dev-1

init: build ## Initialize sequencer configuration
	@echo "⚙️  Initializing Pranklin Sequencer..."
	./bin/pranklin-sequencer init --root-dir ~/.pranklin-sequencer

test: ## Run tests
	@echo "🧪 Running tests..."
	go test -v ./cmd/... ./grpc/...

clean: ## Clean build artifacts
	@echo "🧹 Cleaning..."
	rm -rf bin/
	go clean

verify-http2: ## Verify HTTP/2 support in dependencies
	@echo "🔍 Verifying HTTP/2 support..."
	@go list -f '{{ .Imports }}' ./grpc | grep -q "golang.org/x/net/http2" && echo "✅ HTTP/2 transport available" || echo "❌ HTTP/2 transport missing"

# Helper commands
.PHONY: check-services

check-services: ## Check if required services are running
	@echo "🔍 Checking services..."
	@echo -n "  DA Layer (localhost:7980): "
	@curl -s http://localhost:7980 > /dev/null 2>&1 && echo "✅ Running" || echo "❌ Not running"
	@echo -n "  Pranklin Execution (localhost:50051): "
	@(echo | timeout 1 telnet localhost 50051 2>/dev/null | grep -q "Connected") && echo "✅ Running" || echo "❌ Not running"

