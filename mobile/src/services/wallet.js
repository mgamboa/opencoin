import { getBalance, getAddress, getInfo, sendToAddress, discoverNode } from './rpc';

export async function connectAndFetch() {
  try {
    const discovery = await discoverNode();
    const [balance, address, info] = await Promise.all([
      getBalance(),
      getAddress(),
      getInfo(),
    ]);
    return { info, balance, address, url: discovery.url, error: null };
  } catch (e) {
    return { info: null, balance: null, address: null, url: null, error: e.message };
  }
}

export async function fetchWalletData() {
  try {
    const [info, balance, address] = await Promise.all([
      getInfo(),
      getBalance(),
      getAddress(),
    ]);
    return { info, balance, address, error: null };
  } catch (e) {
    return { info: null, balance: null, address: null, error: e.message };
  }
}

export async function sendCoins(address, amount, fee) {
  try {
    const result = await sendToAddress(address, amount, fee);
    return { success: true, txHash: result.tx_hash || result };
  } catch (e) {
    return { success: false, error: e.message };
  }
}
