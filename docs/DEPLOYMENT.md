# Pranklin Perp DEX - Deployment Guide

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Development Setup](#development-setup)
3. [Docker Deployment](#docker-deployment)
4. [Production Deployment](#production-deployment)
5. [Monitoring](#monitoring)
6. [Backup & Recovery](#backup--recovery)
7. [Troubleshooting](#troubleshooting)

## Prerequisites

### System Requirements

**Minimum:**

- CPU: 4 cores
- RAM: 8 GB
- Storage: 100 GB SSD
- Network: 100 Mbps

**Recommended:**

- CPU: 8+ cores
- RAM: 16+ GB
- Storage: 500 GB NVMe SSD
- Network: 1 Gbps

### Software Requirements

- Rust 1.75+ (for building from source)
- Docker 20.10+ & Docker Compose 2.0+ (for containerized deployment)
- PostgreSQL 15+ (optional, for analytics)

## Development Setup

### Quick Start

```bash
# Clone the repository
git clone <repository-url>
cd pranklin-core

# Run development environment
chmod +x scripts/*.sh
./scripts/start-dev.sh
```

This will:

- Build the project in release mode
- Start Prometheus and Grafana
- Launch the Pranklin node
- Set up logging

### Manual Development Setup

```bash
# Build the project
cargo build --release

# Run the node
RUST_LOG=info,pranklin=debug cargo run --release --bin pranklin-app -- \
    --db-path ./data/db \
    --rpc-addr 0.0.0.0:8545 \
    --grpc-addr 0.0.0.0:26658
```

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run integration tests
cargo test --package pranklin-engine --test integration_tests

# Run benchmarks
./scripts/benchmark.sh
```

## Docker Deployment

### Single Node Deployment

```bash
# Build Docker image
docker build -t pranklin-dex:latest .

# Run container
docker run -d \
    --name pranklin-node \
    -p 8545:8545 \
    -p 26658:26658 \
    -p 9090:9090 \
    -v pranklin-data:/data \
    -e RUST_LOG=info,pranklin=debug \
    pranklin-dex:latest
```

### Docker Compose Deployment

```bash
# Start all services
docker-compose up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f pranklin-node

# Stop services
docker-compose down
```

Services included:

- **pranklin-node**: Main DEX node (ports 8545, 26658, 9090)
- **prometheus**: Metrics collection (port 9091)
- **grafana**: Metrics visualization (port 3000)

## Production Deployment

### System Configuration

#### 1. Increase File Descriptors

```bash
# Add to /etc/security/limits.conf
* soft nofile 65536
* hard nofile 65536
```

#### 2. Configure Swap (if needed)

```bash
sudo fallocate -l 16G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
```

#### 3. Optimize Network Settings

```bash
# Add to /etc/sysctl.conf
net.core.rmem_max = 134217728
net.core.wmem_max = 134217728
net.ipv4.tcp_rmem = 4096 87380 67108864
net.ipv4.tcp_wmem = 4096 65536 67108864
```

### Systemd Service

Create `/etc/systemd/system/pranklin-node.service`:

```ini
[Unit]
Description=Pranklin Perp DEX Node
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pranklin
Group=pranklin
WorkingDirectory=/opt/pranklin
ExecStart=/usr/local/bin/pranklin-app \
    --db-path /var/lib/pranklin/db \
    --rpc-addr 0.0.0.0:8545 \
    --grpc-addr 0.0.0.0:26658
Restart=always
RestartSec=10
LimitNOFILE=65536
Environment="RUST_LOG=info,pranklin=debug"
Environment="RUST_BACKTRACE=1"

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/pranklin

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl enable pranklin-node
sudo systemctl start pranklin-node
sudo systemctl status pranklin-node
```

### Nginx Reverse Proxy

Create `/etc/nginx/sites-available/pranklin-dex`:

```nginx
upstream pranklin_rpc {
    server 127.0.0.1:8545;
    keepalive 64;
}

upstream pranklin_ws {
    server 127.0.0.1:8545;
}

server {
    listen 80;
    listen [::]:80;
    server_name api.pranklin-dex.example.com;

    # Redirect to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name api.pranklin-dex.example.com;

    # SSL configuration
    ssl_certificate /etc/letsencrypt/live/api.pranklin-dex.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.pranklin-dex.example.com/privkey.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers HIGH:!aNULL:!MD5;

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api_limit:10m rate=100r/s;
    limit_req zone=api_limit burst=200 nodelay;

    # HTTP endpoints
    location / {
        proxy_pass http://pranklin_rpc;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_connect_timeout 60s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;
    }

    # WebSocket endpoint
    location /ws {
        proxy_pass http://pranklin_ws;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_connect_timeout 7d;
        proxy_send_timeout 7d;
        proxy_read_timeout 7d;
    }

    # Health check endpoint (no rate limit)
    location /health {
        proxy_pass http://pranklin_rpc/health;
        access_log off;
    }
}
```

Enable and reload:

```bash
sudo ln -s /etc/nginx/sites-available/pranklin-dex /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

## Monitoring

### Prometheus Metrics

Access Prometheus at `http://localhost:9091`

Key metrics to monitor:

- `pranklin_tx_submitted_total`: Total transactions submitted
- `pranklin_tx_processed_total`: Successfully processed transactions
- `pranklin_orders_placed_total`: Orders placed by market
- `pranklin_liquidations_total`: Liquidation events
- `pranklin_request_duration_seconds`: RPC latency
- `pranklin_mempool_size`: Pending transactions
- `pranklin_block_height`: Current block height

### Grafana Dashboards

Access Grafana at `http://localhost:3000` (default credentials: admin/admin)

Recommended dashboards:

1. **Node Overview**: Health, block height, TPS
2. **Trading Activity**: Orders, fills, volume
3. **Risk Metrics**: Liquidations, margin usage
4. **Performance**: Latency, throughput, resource usage

### Alerts

Configure alerts in `monitoring/alerts.yml`:

- High transaction failure rate
- Circuit breaker triggered
- Mempool growing
- High liquidation rate
- RPC latency spikes

## Backup & Recovery

### Database Backup

```bash
# Manual snapshot
curl -X POST http://localhost:8545/admin/snapshot

# Automated backup script
#!/bin/bash
BACKUP_DIR=/backup/pranklin
DATE=$(date +%Y%m%d_%H%M%S)

# Stop node (if needed for consistency)
systemctl stop pranklin-node

# Backup database
tar -czf $BACKUP_DIR/db_backup_$DATE.tar.gz /var/lib/pranklin/db/

# Restart node
systemctl start pranklin-node

# Keep last 7 days of backups
find $BACKUP_DIR -name "db_backup_*.tar.gz" -mtime +7 -delete
```

### State Recovery

```bash
# Stop node
systemctl stop pranklin-node

# Restore from backup
tar -xzf /backup/pranklin/db_backup_YYYYMMDD.tar.gz -C /var/lib/pranklin/

# Start node
systemctl start pranklin-node
```

### Snapshot Sync (Fast Sync)

```bash
# Download latest snapshot
wget https://snapshots.pranklin-dex.example.com/latest.tar.lz4

# Extract
lz4 -d latest.tar.lz4 | tar -xf - -C /var/lib/pranklin/db/

# Start node
systemctl start pranklin-node
```

## Troubleshooting

### Common Issues

#### 1. Node Won't Start

```bash
# Check logs
journalctl -u pranklin-node -f

# Verify database permissions
chown -R pranklin:pranklin /var/lib/pranklin

# Check port availability
netstat -tulpn | grep -E '8545|26658|9090'
```

#### 2. High Memory Usage

```bash
# Monitor memory
free -h
ps aux | grep pranklin-app

# Adjust RocksDB cache size (in config)
# reduce block_cache_size if needed
```

#### 3. Database Corruption

```bash
# Stop node
systemctl stop pranklin-node

# Verify database integrity
# (implement db check tool)

# Restore from backup if needed
```

#### 4. Network Issues

```bash
# Test connectivity
curl http://localhost:8545/health

# Check firewall
sudo ufw status

# Verify nginx configuration
sudo nginx -t
```

### Performance Tuning

#### 1. RocksDB Optimization

Adjust in code or config:

- `block_cache_size`: Increase for more RAM
- `write_buffer_size`: Increase for write-heavy loads
- `max_open_files`: Increase file descriptor limit

#### 2. Rate Limiting

Adjust in `crates/rpc/src/lib.rs`:

```rust
let rate_limiter = RateLimitLayer::new(1000); // requests per second
```

#### 3. Connection Pooling

For high load, increase connection limits in nginx and systemd configs.

## Security Checklist

- [ ] Enable HTTPS with valid SSL certificates
- [ ] Configure firewall rules
- [ ] Set up rate limiting
- [ ] Enable monitoring and alerts
- [ ] Regular security updates
- [ ] Backup encryption
- [ ] Access control and authentication
- [ ] Regular security audits
- [ ] DDoS protection
- [ ] Intrusion detection

## Support

For issues and questions:

- GitHub Issues: <repository-url>/issues
- Documentation: <docs-url>
- Discord: <discord-invite>
