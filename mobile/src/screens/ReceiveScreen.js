import React, { useState, useEffect } from 'react';
import { View, Text, StyleSheet, ActivityIndicator } from 'react-native';
import QRCode from 'react-native-qrcode-svg';
import { loadWallet } from '../services/localwallet';

export default function ReceiveScreen() {
  const [address, setAddress] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    (async () => {
      const w = await loadWallet();
      if (w) setAddress(w.address);
      setLoading(false);
    })();
  }, []);

  if (loading) {
    return (
      <View style={styles.centered}>
        <ActivityIndicator size="large" color="#f7931a" />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Receive OPC</Text>
      <View style={styles.qrContainer}>
        {address && (
          <QRCode value={address} size={240} backgroundColor="#0d1117" color="#f7931a" />
        )}
      </View>
      <Text style={styles.addressLabel}>Your Address</Text>
      <Text style={styles.address} selectable>{address || 'No wallet'}</Text>
      <Text style={styles.hint}>Share this address to receive payments</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#0d1117', alignItems: 'center', padding: 20 },
  centered: { flex: 1, justifyContent: 'center', alignItems: 'center', backgroundColor: '#0d1117' },
  title: { color: '#c9d1d9', fontSize: 22, fontWeight: '600', marginTop: 20 },
  qrContainer: { padding: 20, backgroundColor: '#161b22', borderRadius: 16, marginTop: 24, borderWidth: 1, borderColor: '#30363d' },
  addressLabel: { color: '#8b949e', fontSize: 14, marginTop: 24, marginBottom: 8 },
  address: { color: '#58a6ff', fontSize: 13, fontFamily: 'monospace', textAlign: 'center', padding: 12, backgroundColor: '#161b22', borderRadius: 8, borderWidth: 1, borderColor: '#30363d', width: '100%' },
  hint: { color: '#8b949e', fontSize: 13, marginTop: 16, textAlign: 'center' },
});
