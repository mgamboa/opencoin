import * as SecureStore from 'expo-secure-store';
import { generateKeypair, secretToKeypair, deriveAddress, bytesToHex } from './crypto';
import { rpcCall } from './rpc';

const WALLET_KEY = 'opencoin_wallet';

export async function createWallet() {
  const kp = generateKeypair();
  const address = deriveAddress(kp.publicHex);
  const walletData = {
    secretHex: kp.secretHex,
    publicHex: kp.publicHex,
    address,
  };
  await SecureStore.setItemAsync(WALLET_KEY, JSON.stringify(walletData));
  return walletData;
}

export async function importWallet(secretHex) {
  const kp = secretToKeypair(secretHex);
  const address = deriveAddress(kp.publicHex);
  const walletData = {
    secretHex: kp.secretHex,
    publicHex: kp.publicHex,
    address,
  };
  await SecureStore.setItemAsync(WALLET_KEY, JSON.stringify(walletData));
  return walletData;
}

export async function loadWallet() {
  try {
    const data = await SecureStore.getItemAsync(WALLET_KEY);
    if (!data) return null;
    return JSON.parse(data);
  } catch {
    return null;
  }
}

export async function hasWallet() {
  const w = await loadWallet();
  return w !== null;
}

export async function clearWallet() {
  await SecureStore.deleteItemAsync(WALLET_KEY);
}

export async function getWalletBalance() {
  const w = await loadWallet();
  if (!w) return null;
  try {
    const result = await rpcCall('getaddressbalance', [w.publicHex]);
    return { balance: result.balance, utxoCount: result.utxo_count };
  } catch (e) {
    return { balance: 0, utxoCount: 0, error: e.message };
  }
}

export async function sendFromWallet(toAddress, amount, fee) {
  const w = await loadWallet();
  if (!w) return { success: false, error: 'No wallet loaded' };
  try {
    const result = await rpcCall('sendwithkey', [w.secretHex, toAddress, amount, fee || 10]);
    return { success: true, txHash: result.tx_hash };
  } catch (e) {
    return { success: false, error: e.message };
  }
}
