import nacl from 'tweetnacl';
import { sha3_256 } from 'js-sha3';

function doubleSha3(data) {
  return sha3_256(sha3_256(data));
}

export function generateKeypair() {
  const seed = nacl.randomBytes(32);
  const kp = nacl.sign.keyPair.fromSeed(seed);
  return {
    secretKey: seed,
    publicKey: kp.publicKey,
    secretHex: bytesToHex(seed),
    publicHex: bytesToHex(kp.publicKey),
  };
}

export function publicKeyToAddress(pubkeyBytes) {
  let addr = 'OC' + bytesToHex(pubkeyBytes);
  const checksum = doubleSha3(addr);
  addr += bytesToHex(checksum.slice(0, 4));
  return addr;
}

export function deriveAddress(publicHex) {
  const pubkey = hexToBytes(publicHex);
  return publicKeyToAddress(pubkey);
}

export function secretToKeypair(secretHex) {
  const seed = hexToBytes(secretHex);
  const kp = nacl.sign.keyPair.fromSeed(seed);
  return {
    secretKey: seed,
    publicKey: kp.publicKey,
    secretHex,
    publicHex: bytesToHex(kp.publicKey),
  };
}

export function bytesToHex(bytes) {
  return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
}

export function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substr(i, 2), 16);
  }
  return bytes;
}
