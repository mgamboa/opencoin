import seeds from '../../seeds.json';

let seedsList = [...seeds];
let nodeUrl = seedsList[0] || 'http://localhost:9769';

export function setNodeUrl(url) {
  nodeUrl = url;
  if (!seedsList.includes(url)) {
    seedsList.unshift(url);
  }
}

export function getNodeUrl() {
  return nodeUrl;
}

export async function discoverNode() {
  for (const url of seedsList) {
    try {
      const resp = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'getinfo', params: [] }),
        signal: AbortSignal.timeout(3000),
      });
      const data = await resp.json();
      if (data && data.result) {
        nodeUrl = url;
        return { url, info: data.result };
      }
    } catch (_) {}
  }
  throw new Error('No reachable node found');
}

export async function rpcCall(method, params = []) {
  const response = await fetch(nodeUrl, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: 1,
      method,
      params,
    }),
    signal: AbortSignal.timeout(5000),
  });
  const data = await response.json();
  if (data.error) throw new Error(data.error.message || JSON.stringify(data.error));
  return data.result;
}

export async function getBalance() {
  return rpcCall('getbalance');
}

export async function getAddress() {
  return rpcCall('getaddress');
}

export async function getInfo() {
  return rpcCall('getinfo');
}

export async function sendToAddress(address, amount, fee) {
  const params = [address, amount];
  if (fee) params.push(fee);
  return rpcCall('sendtoaddress', params);
}

export async function getTransaction(txHash) {
  return rpcCall('gettransaction', [txHash]);
}

export async function getBlock(height) {
  return rpcCall('getblock', [height]);
}

export async function getBlockCount() {
  return rpcCall('getblockcount');
}

export async function sendRawTransaction(txHex) {
  return rpcCall('sendrawtransaction', [txHex]);
}

export async function getAddressBalance(pubkeyHex) {
  return rpcCall('getaddressbalance', [pubkeyHex]);
}

export async function sendWithKey(secretHex, toAddress, amount, fee) {
  return rpcCall('sendwithkey', [secretHex, toAddress, amount, fee || 10]);
}
