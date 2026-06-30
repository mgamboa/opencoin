# OpenCoin Exchange Integration Guide v1.0

## Overview

OpenCoin (OPC) is a privacy-focused cryptocurrency with RingCT, stealth addresses,
WASM smart contracts, and Bitcoin-compatible JSON-RPC. This guide covers everything
an exchange needs to integrate OPC deposits and withdrawals.

**Network**: Mainnet  
**Protocol**: Custom P2P (TCP-based, length-prefixed bincode)  
**RPC Port**: 8545 (JSON-RPC over HTTP)  
**P2P Port**: 8535 (Seed: 192.168.15.4)  
**Consensus**: Blake3-PoW (CPU-friendly)  
**Privacy**: RingCT (optional) + Stealth Addresses

---

## Running a Node

### Requirements
- Linux x86_64 or aarch64
- No special hardware required (CPU mining)

### Installation
```bash
# Download binary
wget <release-url>/opencoin-node
chmod +x opencoin-node

# Create wallet
./opencoin-node --wallet-dir /data/opc-wallet

# Run
./opencoin-node --seed 192.168.15.4:8535 \
  --rpc-port 8545 \
  --data-dir /data/opc-blockchain \
  --wallet-dir /data/opc-wallet
```

### Configuration
| Flag | Default | Description |
|------|---------|-------------|
| `--seed` | - | Seed node address for peer discovery |
| `--rpc-port` | 8545 | JSON-RPC server port |
| `--p2p-port` | 8535 | P2P network port |
| `--data-dir` | ~/.opencoin/data | Blockchain storage directory |
| `--wallet-dir` | ~/.opencoin/wallets | Wallet storage directory |
| `--pool` | - | Enable mining pool server on :8536 |

---

## RPC API

All RPC methods use standard JSON-RPC 2.0 over HTTP POST.

**Request Format:**
```json
{"jsonrpc":"2.0","id":1,"method":"method_name","params":[...]}
```

**Response Format:**
```json
{"jsonrpc":"2.0","id":1,"result":{...}}
```

### Wallet Methods

#### `getbalance`
Get the node wallet's balance.
**Params:** none
**Response:**
```json
{
  "balance": 100000000000,
  "locked": 0,
  "address": "oc1abc123..."
}
```

#### `getaddress`
Get the node wallet's receive address.
**Params:** none
**Response:** `{"address": "oc1abc123..."}`

#### `sendtoaddress`
Send OPC to an address.
**Params:** `[address, amount, fee?]`
- `address`: string — recipient OpenCoin address (oc1...)
- `amount`: u64 — amount in atomic units (1 OPC = 100,000,000 units)
- `fee`: u64 — optional fee (default: 0.0001 OPC)
**Response:** `{"tx_hash": "abc123..."}`

### Blockchain Methods

#### `getblockcount`
Get current blockchain height.
**Params:** none
**Response:** `{"blocks": 12345}`

#### `getblockhash`
Get block hash by height.
**Params:** `[height]`
**Response:** `{"hash": "abc123..."}`

#### `getblockheader`
Get block header by height.
**Params:** `[height]`
**Response:**
```json
{
  "hash": "abc123...",
  "height": 12345,
  "version": 1,
  "timestamp": 1718000000,
  "previous_hash": "def456...",
  "merkle_root": "789abc..."
}
```

#### `getblock`
Get block info by height.
**Params:** `[height]`
**Response:**
```json
{
  "height": 12345,
  "hash": "abc123...",
  "timestamp": 1718000000,
  "tx_count": 25
}
```

### Transaction Methods

#### `gettransaction`
Get transaction details.
**Params:** `[tx_hash_hex]`
**Response:**
```json
{
  "tx_hash": "abc123...",
  "version": 1,
  "tx_type": "Regular",
  "inputs": [...],
  "outputs": [...],
  "fee": 10000,
  "timestamp": 1718000000,
  "confirmations": 100
}
```

#### `sendrawtransaction`
Broadcast a pre-built transaction.
**Params:** `[hex_encoded_raw_tx]`
**Response:** `{"tx_hash": "abc123..."}`

#### `getrawtransaction`
Get raw transaction bytes (hex-encoded).
**Params:** `[tx_hash_hex]`
**Response:** `{"raw": "abc123..."}`

### Address Methods

#### `validateaddress`
Validate an OpenCoin address.
**Params:** `[address_string]`
**Response:**
```json
{
  "valid": true,
  "address": "oc1abc123...",
  "type": "stealth"
}
```

### Mining Methods

#### `getmininginfo`
Get mining/block template info.
**Params:** none
**Response:**
```json
{
  "height": 12345,
  "difficulty": 123456,
  "reward": 5000000000,
  "transactions": 10
}
```

---

## Deposit Workflow

```
1. User provides address     → exchange generates oc1 address
2. Exchange monitors blockchain → scan blocks for outputs to known addresses
3. Confirmations              → wait for N blocks (recommended: 6)
4. Credit user account        → update internal ledger
```

### Step 1: Generate Deposit Address
```bash
# Generate a new address for each user
curl -X POST http://localhost:8545 \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getaddress","params":[]}'
```

### Step 2: Monitor for Deposits
OpenCoin uses stealth addresses — each transaction output is unique per sender.
An exchange must scan blockchain blocks and check if outputs belong to known
deposit addresses using the node wallet.

**Recommended approach:** Run the exchange node with `--wallet-dir` pointing to
a wallet containing all deposit addresses. The node automatically scans incoming
blocks and credits the wallet balance.

### Step 3: Confirmations
Block time: ~30 seconds. Recommended confirmations: **6 blocks** (~3 minutes).
Configure via `--confirmations N` on the exchange side.

---

## Withdrawal Workflow

### Create and Broadcast Transaction
```bash
curl -X POST http://localhost:8545 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":1,
    "method":"sendtoaddress",
    "params":["oc1xxxx...", 10000000000, 10000]
  }'
```

### Monitor Transaction Status
```bash
curl -X POST http://localhost:8545 \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc":"2.0",
    "id":1,
    "method":"gettransaction",
    "params":["tx_hash_hex"]
  }'
```

---

## Address Format

OpenCoin addresses use Bech32-style encoding: `oc1` prefix followed by
a base32-encoded stealth address (spend key + view key).

**Example:** `oc1qypqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqzgv9k5`

**Validation:** Use `validateaddress` RPC method.

---

## Security Considerations

1. **Run a dedicated node** — do not share the node with other services
2. **Firewall the RPC port** — bind to `127.0.0.1` in production:
   ```bash
   ./opencoin-node --rpc-port 8545 --rpc-bind 127.0.0.1
   ```
3. **Wallet security** — back up the wallet directory regularly
4. **Minimum confirmations** — use 6+ confirmations for deposits
5. **Rate limiting** — apply rate limiting on RPC endpoints
6. **Monitoring** — monitor node health, disk usage, and sync status

---

## Support

- GitHub: https://github.com/anomalyco/opencoin
- Node status: `/getinfo` RPC method returns height, supply, version
