# OpenCoin

A decentralized, mineable cryptocurrency for everyday use. CPU-friendly, ASIC-resistant, private by default with optional transparency via view keys.

**Status:** Mainnet live since June 29, 2026.

## Features

- **PoW Mining** — CPU-friendly, ASIC-resistant (RandomX-style)
- **Privacy** — Stealth addresses + ring signatures by default
- **View Keys** — Optional transparency for auditing
- **Smart Contracts** — WASM-based (in development)
- **P2P Network** — Decentralized peer-to-peer
- **RPC API** — JSON-RPC + Web dashboard

## Tokenomics

| Parameter | Value |
|---|---|
| Total Supply | 8,000,000,000 OC |
| Premine (Mario) | 20,000,000 OC (0.25%) |
| Block Time | 120 seconds (2 minutes) |
| Initial Block Reward | 304 OC per block |
| Halving Interval | Every 27,382,500 blocks (~50 years) |
| Emission Completion | ~300 years |
| Decimals | 12 |

### Block Reward Schedule

| Period | Blocks | Reward/Block | Total Mined |
|---|---|---|---|
| Year 1-50 | 0 - 27.3M | 304 OC | ~4.16B OC |
| Year 51-100 | 27.3M - 54.8M | 152 OC | ~2.08B OC |
| Year 101-150 | 54.8M - 82.1M | 76 OC | ~1.04B OC |
| Year 151-200 | 82.1M - 109.5M | 38 OC | ~0.52B OC |
| Year 201-250 | 109.5M - 136.9M | 19 OC | ~0.26B OC |
| Year 251+ | 136.9M+ | Declining to min. 1 | Remainder |

### How Miners Earn

Each mined block creates a **coinbase transaction** that sends the block reward to the miner's wallet address. Solo miners receive 100% of the block reward.

### Mining Pools (Coming Soon)

Pool operators will run a pool server that:
1. Distributes work to miners (lower-difficulty shares)
2. Tracks shares submitted by each miner
3. When a pool miner finds a block, the reward is distributed proportionally
4. Pool operators can take a fee (typically 1-2%)

## Prerequisites

- **Rust** 1.70+ ([install](https://rustup.rs))
- **Ubuntu/Debian:** `sudo apt install build-essential pkg-config libssl-dev clang`
- **Arch Linux:** `sudo pacman -S base-devel openssl clang`
- **macOS:** `xcode-select --install`

## Build

```bash
git clone https://github.com/mgamboa/opencoin.git
cd opencoin
cargo build --release
```

The build produces three binaries in `target/release/`:
- `opencoin-node` — Full blockchain node (with optional mining)
- `opencoin-wallet` — CLI wallet
- `opencoin-miner` — Standalone miner

## Run a Node

### Start a mining node (with your own wallet):

```bash
./target/release/opencoin-node start --mine
```

This generates a new keypair on first run. **SAVE the secret key** shown in the logs.

### Start a mining node with a specific key:

```bash
./target/release/opencoin-node start --mine --premine-key <your_64_byte_hex_secret_key>
```

### Start a node and connect to a peer:

```bash
./target/release/opencoin-node start --peer 192.168.2.10:9768
```

### Start a node as a miner connected to a pool node:

```bash
./target/release/opencoin-node start --mine --peer <pool_ip>:9768
```

### Start a non-mining node (relay):

```bash
./target/release/opencoin-node start
```

### Data directory:

By default, data is stored in `~/.opencoin/`. Override with:
```bash
./target/release/opencoin-node start --data-dir /path/to/data --mine
```

## Connected Mining (Solo)

When you run `opencoin-node start --mine`, the node:
1. Creates a new block every ~2 minutes (at target difficulty)
2. Includes a coinbase transaction sending the block reward to your wallet
3. Broadcasts the block to all connected peers
4. Continues mining the next block

The wallet balance is tracked locally. Initially, difficulty is 1 (instant mining). As the chain grows, difficulty adjusts to maintain 2-minute block intervals.

## RPC API

The node exposes a JSON-RPC 2.0 API on `http://<node_ip>:9769/`.

### Endpoints:

**GET `/`** — Web dashboard (auto-refreshes)

**POST `/` or `/rpc` or `/api`** — JSON-RPC

### curl examples:

```bash
# Get blockchain info
curl -s http://localhost:9769/ -d '{"method":"getinfo","params":[],"id":1}'

# Get block by height
curl -s http://localhost:9769/ -d '{"method":"getblock","params":[0],"id":1}'

# Get wallet balance
curl -s http://localhost:9769/ -d '{"method":"getbalance","params":[],"id":1}'

# Get wallet address
curl -s http://localhost:9769/ -d '{"method":"getaddress","params":[],"id":1}'
```

### RPC Methods:

| Method | Params | Returns |
|---|---|---|
| `getinfo` | none | height, supply, version |
| `getblock` | [height] | block hash, timestamp, tx count |
| `getbalance` | none | balance, locked, address |
| `getaddress` | none | wallet address |

## Network

- **P2P Port:** 9768 (TCP)
- **RPC Port:** 9769 (TCP)
- **Protocol:** TCP with length-prefixed bincode messages
- **Message Types:** Ping/Pong, Block, Transaction, GetBlocks/Blocks, GetPeers/Peers

### Port Forwarding

For external nodes to connect to yours, forward ports on your router:
- `9768` TCP → your node's IP
- `9769` TCP → your node's IP (optional, for RPC access)

## Building from Source (All Platforms)

OpenCoin is tested on:
- **x86-64** (Ubuntu 24.04, Rust 1.96+) — confirmed working
- **ARM64 / aarch64** (Raspberry Pi 4/5, Ubuntu) — confirmed working
- **macOS** (should work, untested)
- **Windows** (via WSL2, untested)

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Install system deps (Ubuntu/Debian)
sudo apt update && sudo apt install build-essential pkg-config libssl-dev clang git -y

# 3. Clone and build
git clone https://github.com/mgamboa/opencoin.git
cd opencoin
cargo build --release

# 4. Run
./target/release/opencoin-node start --mine
```

## Wallet Management

### CLI Wallet:

```bash
# Create a new wallet
./target/release/opencoin-wallet create

# Show wallet info
./target/release/opencoin-wallet show

# Generate a keypair
./target/release/opencoin-wallet generate-key

# Validate an address
./target/release/opencoin-wallet validate OCabc123...
```

### Address Format:

Addresses start with `OC` followed by 60 hex chars (30 bytes blake3 hash + 4 bytes checksum).

## Privacy Model

OpenCoin uses **stealth addresses**:
- Each transaction output is a unique one-time address derived from the recipient's public key + sender's ephemeral key
- Only the recipient can detect incoming payments using their private key
- **View keys** allow designated third parties to view a wallet's transactions without spending authority

## License

MIT

## Roadmap

- [x] Mainnet launch (June 2026)
- [x] Solo CPU mining
- [x] P2P network with block sync
- [x] Web dashboard + RPC
- [ ] Mining pool server
- [ ] WASM smart contracts
- [ ] RingCT (confidential transactions)
- [ ] Mobile wallet
- [ ] Exchange listings / CoinMarketCap
- [ ] Light client (SPV)

## Contributing

Open an issue or PR at [github.com/mgamboa/opencoin](https://github.com/mgamboa/opencoin).

## Disclaimer

Cryptocurrency involves risk. OpenCoin is experimental software. Use at your own risk.
