import { getBalance, getAddress, getInfo, sendToAddress } from './rpc';

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
