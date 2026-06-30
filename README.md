# OpenCoin

A decentralized, mineable cryptocurrency for everyday use. CPU-friendly, ASIC-resistant, private by default with optional transparency via view keys.

**Status:** Mainnet live since June 29, 2026.

## Features

- **PoW Mining** — CPU-friendly, ASIC-resistant (RandomX-style)
- **Privacy** — Stealth addresses + ring signatures by default
- **Confidential Transactions (RingCT)** — Hidden amounts with Pedersen commitments, range proofs, AOS ring signatures
- **View Keys** — Optional transparency for auditing
- **WASM Smart Contracts** — WebAssembly-based contracts with wasmtime runtime, fuel metering, and persistent KV storage
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

### Mining Pools

Pool operators run a pool server that:
1. Distributes work to miners (lower-difficulty shares)
2. Tracks shares submitted by each miner
3. When a pool miner finds a block, the reward is distributed proportionally
4. Pool operators can take a fee (typically 1-2%)

**Pool server is built into the node.** Start with `--pool` flag.

### Decentralized Discovery

New nodes find the network through a **bootstrap peer list** in the git repository:

```bash
# peers.json contains known public nodes
cat peers.json

# The miner can auto-discover a live pool from this list:
opencoin-miner --discover --address YOUR-OC-ADDRESS --threads 4

# Or connect to a specific pool:
opencoin-miner --pool any-public-node:3333 --address YOUR-OC-ADDRESS --threads 4
```

As the network grows, the peer list expands. Submit a PR to add your public node to `peers.json`.

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

## Pool Server

Start a node with pool support:

```bash
./target/release/opencoin-node start --pool --pool-port 3333
```

This generates a pool wallet keypair. **SAVE the pool secret key** to collect rewards.

### Connect a miner

```bash
# Clone and build on another machine, then:
./target/release/opencoin-miner --pool <pool_ip>:3333 --threads 4
```

The miner connects to the pool, receives work units, and submits shares when it finds hashes below the share target.

### Pool Configuration

| Flag | Default | Description |
|---|---|---|
| `--pool` | false | Enable pool server |
| `--pool-port` | 3333 | Pool TCP port |
| `--pool-address` | (auto) | Pool wallet hex public key |

### Pool Protocol

The pool uses JSON-line TCP protocol:

**Pool → Miner (Job):**
```json
{"type":"job","job_id":1,"height":1,"target":18446744073709551615,"share_target":18446744073709551,"header":"01000000..."}
```

**Miner → Pool (Submit):**
```json
{"type":"submit","job_id":1,"nonce":12345,"thread":0}
```

**Pool → Miner (Result):**
```json
{"type":"result","job_id":1,"nonce":12345,"status":"accepted","message":"Share accepted"}
```

### Miner Performance

On a modern x86-64 CPU, each thread achieves ~1.1 MH/s. On Raspberry Pi 4 (ARM64), expect ~100-200 KH/s per thread.

## Web Dashboard

The node includes a web interface at `http://<node_ip>:9769/`.

### Pages

| Path | Description |
|---|---|
| `/` | Blockchain dashboard (height, supply, peers) |
| `/wallet` | Wallet balance and address |
| `/blocks` | Recent blocks list |
| `/pool` | Pool status, connected miners, shares |

The dashboard auto-refreshes every 2 seconds.

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
| `getpoolstats` | none | pool miners, shares, current job |
| `sendtoaddress` | [address, amount, fee?] | tx_hash, amount, fee, change |
| `deploycontract` | [code_hex, args_hex, fee?] | tx_hash, contract_address, code_size |
| `callcontract` | [address_hex, function, args_hex, fee?] | tx_hash, contract_address, function |
| `callcontractview` | [address_hex, function, args_hex] | gas_used, result, events |
| `getcontractstate` | [address_hex] | contract_address, state (key-value map) |

## Network

- **P2P Port:** 9768 (TCP)
- **RPC Port:** 9769 (TCP)
- **Protocol:** TCP with length-prefixed bincode messages
- **Message Types:** Ping/Pong, Block, Transaction, GetBlocks/Blocks, GetPeers/Peers, MempoolRequest/MempoolResponse

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

# 4. Run a node (discovers peers from peers.json automatically):
./target/release/opencoin-node start

# Or connect to a specific seed node:
./target/release/opencoin-node start --seed any-public-node:9768

# Run a mining node with your wallet:
./target/release/opencoin-node start --mine --premine-key YOUR-SECRET-KEY

# Run a pool server:
./target/release/opencoin-node start --pool --pool-port 3333

# Connect a miner to the pool (auto-discover from peers.json):
./target/release/opencoin-miner --discover --address YOUR-OC-ADDRESS --threads 4

# Or connect to a specific pool:
./target/release/opencoin-miner --pool pool-host:3333 --address YOUR-OC-ADDRESS --threads 4
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

### Wallet Recovery (⚠️ Critical)

**Your wallet IS your secret key.** If your machine dies, you can recover everything from the hex key.

```bash
# Recover your wallet by starting a node with your secret key:
./target/release/opencoin-node start --premine-key YOUR-64-BYTE-HEX-SECRET-KEY

# Your balance, address, and transaction history will be restored.
# The blockchain is synced from the network; your coins are on-chain.
```

**Save your secret key offline** (paper wallet, encrypted USB). Without it, your coins are gone forever.

### Address Format:

Addresses start with `OC` followed by 64 hex chars (32 bytes public key) + 8 hex chars (4 bytes checksum) — 74 characters total.

## Smart Contracts

OpenCoin supports **WASM smart contracts** compiled from any language that targets WebAssembly (Rust, C, C++, Go, etc.).

### Contract Interface

Contracts export two functions:
- `init(args_ptr: i32, args_len: i32) -> i32` — constructor, called on deploy
- `call(args_ptr: i32, args_len: i32) -> i32` — entry point for contract calls

Host functions imported from `env`:
- `read_storage(key_ptr, key_len, val_ptr, max_len) -> u32` — read from KV store
- `write_storage(key_ptr, key_len, val_ptr, val_len)` — write to KV store
- `get_caller(ptr, max_len) -> u32` — get caller address
- `get_block_height() -> i64` — current block height
- `get_contract_address(ptr, max_len) -> u32` — this contract's address
- `emit_event(data_ptr, data_len)` — emit an event
- `debug_log(ptr, len)` — log a message

### Deploy a Contract

```bash
# Deploy WASM bytecode to the blockchain
curl -s http://localhost:9769/ -d '{
  "method":"deploycontract",
  "params":["<wasm_hex>", "<constructor_args_hex>", 1000],
  "id":1
}'
```

### Call a Contract

```bash
# Call a function on a deployed contract
curl -s http://localhost:9769/ -d '{
  "method":"callcontract",
  "params":["<contract_address_hex>", "my_function", "<args_hex>", 1000],
  "id":1
}'
```

### View Contract State

```bash
# Read-only call (no transaction created)
curl -s http://localhost:9769/ -d '{
  "method":"callcontractview",
  "params":["<contract_address_hex>", "my_function", "<args_hex>"],
  "id":1
}'

# Read contract storage state
curl -s http://localhost:9769/ -d '{
  "method":"getcontractstate",
  "params":["<contract_address_hex>"],
  "id":1
}'
```

### Gas

- **Deploy gas limit:** 200,000 fuel units
- **Call gas limit:** 100,000 fuel units
- Gas is metered via wasmtime's built-in fuel mechanism
- Each WASM instruction consumes 1 fuel unit

## Confidential Transactions (RingCT)

OpenCoin supports **RingCT** (Ring Confidential Transactions) that hide transaction amounts:
- **Pedersen commitments** — commit to an amount without revealing it: `C = x*G + a*H`
- **Range proofs** — 64-bit bit-by-bit proofs proving amounts are non-negative
- **AOS ring signatures** — hide the sender among a ring of decoy outputs
- **Key images** — prevent double-spending within the ring

Create RingCT transactions via RPC (TBD) or programmatically via `Transaction::transfer_private()`.

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
- [x] Mining pool server
- [x] Web wallet + pool dashboard
- [x] Block persistence (save/load chain from disk)
- [x] Wallet persistence (balance survives restarts)
- [x] P2P block sync + peer discovery
- [x] Wallet-to-wallet transfers (sendtoaddress RPC)
- [x] Transaction signing (ed25519 signatures)
- [x] UTXO tracking (balance derived from unspent outputs)
- [x] Mempool in blocks (pending txs included in mined blocks)
- [x] Difficulty adjustment (median-based retargeting)
- [x] Fee market (higher-fee txs prioritized)
- [x] Chain reorg handling
- [x] Block explorer with tx details
- [x] WASM smart contracts (wasmtime, fuel metering, persistent KV storage)
- [x] RingCT (confidential transactions — Pedersen commitments, range proofs, ring signatures)
- [ ] Mobile wallet
- [ ] Exchange listings / CoinMarketCap
- [ ] Light client (SPV)

## Contributing

Open an issue or PR at [github.com/mgamboa/opencoin](https://github.com/mgamboa/opencoin).

## Disclaimer

Cryptocurrency involves risk. OpenCoin is experimental software. Use at your own risk.
