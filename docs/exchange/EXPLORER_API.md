# OpenCoin Block Explorer API v1.0

Lightweight REST API for blockchain explorers. Uses the same JSON-RPC interface
as the exchange API, with explorer-specific endpoints.

## Endpoints

All endpoints: `POST http://<node>:8545`

### `getinfo` — Node overview
```json
{
  "height": 12345,
  "circulating_supply": 50000000000000,
  "total_work": "12345678901234567890",
  "version": "0.1.0",
  "protocol": 1
}
```

### `getblock` — Block details
**Params:** `[height]`
```json
{
  "height": 12345,
  "hash": "abc123...",
  "timestamp": 1718000000,
  "tx_count": 25
}
```

### `getblockheader` — Full block header
**Params:** `[height]`
```json
{
  "hash": "abc123...",
  "height": 12345,
  "version": 1,
  "timestamp": 1718000000,
  "previous_hash": "def456...",
  "merkle_root": "789abc...",
  "difficulty_target": 553713663,
  "nonce": 123456,
  "extra_nonce": 789012
}
```

### `getblockhash` — Block hash lookup
**Params:** `[height]` → `{"hash": "abc123..."}`

### `getblockcount` — Current height
**Params:** none → `{"blocks": 12345}`

### `gettransaction` — Transaction details
**Params:** `[tx_hash_hex]`
```json
{
  "tx_hash": "abc123...",
  "version": 1,
  "tx_type": "Regular",
  "inputs": [{"outpoint": {...}, "key_image": "..."}],
  "outputs": [{"amount": 10000000000, "stealth_address": {...}}],
  "fee": 10000,
  "timestamp": 1718000000,
  "confirmations": 100
}
```

### `validateaddress` — Address validation
**Params:** `[address_string]`
```json
{
  "valid": true,
  "address": "oc1abc123...",
  "type": "stealth"
}
```

## Rate Limiting
- Max 10 requests/second per IP (configurable)
- Use `X-Forwarded-For` for proxy support

## CORS
- All endpoints return `Access-Control-Allow-Origin: *`
