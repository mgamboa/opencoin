const DEFAULT_NODE_URL = 'http://192.168.2.10:8545';

let nodeUrl = DEFAULT_NODE_URL;

export function setNodeUrl(url) {
  nodeUrl = url;
}

export function getNodeUrl() {
  return nodeUrl;
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
  });
  const data = await response.json();
  if (data.error) throw new Error(data.error);
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
