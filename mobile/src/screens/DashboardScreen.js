import React, { useState, useEffect, useCallback } from 'react';
import {
  View, Text, StyleSheet, TouchableOpacity, RefreshControl, ScrollView,
  ActivityIndicator,
} from 'react-native';
import { loadWallet } from '../services/localwallet';
import { getWalletBalance } from '../services/localwallet';
import { getNodeUrl } from '../services/rpc';
import { rpcCall } from '../services/rpc';

export default function DashboardScreen({ navigation }) {
  const [wallet, setWallet] = useState(null);
  const [balance, setBalance] = useState(null);
  const [chainInfo, setChainInfo] = useState(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    try {
      const w = await loadWallet();
      setWallet(w);
      const bal = await getWalletBalance();
      setBalance(bal);
      const info = await rpcCall('getblockchaininfo');
      setChainInfo(info);
    } catch (e) {}
    setLoading(false);
    setRefreshing(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  const onRefresh = () => {
    setRefreshing(true);
    load();
  };

  if (loading) {
    return (
      <View style={styles.centered}>
        <ActivityIndicator size="large" color="#f7931a" />
        <Text style={styles.loadingText}>Loading wallet...</Text>
      </View>
    );
  }

  return (
    <ScrollView
      style={styles.container}
      refreshControl={<RefreshControl refreshing={refreshing} onRefresh={onRefresh} />}
    >
      <View style={styles.card}>
        <Text style={styles.balanceLabel}>Balance</Text>
        <Text style={styles.balanceAmount}>
          {balance ? (balance.balance / 1e8).toFixed(4) : '0.0000'} OPC
        </Text>
        {balance && balance.utxoCount > 0 && (
          <Text style={styles.lockedText}>{balance.utxoCount} UTXO{(balance.utxoCount > 1 ? 's' : '')}</Text>
        )}
      </View>
      <View style={styles.card}>
        <Text style={styles.label}>Address</Text>
        <Text style={styles.address} numberOfLines={2} selectable>
          {wallet ? wallet.address : 'No wallet'}
        </Text>
      </View>
      <View style={styles.card}>
        <Text style={styles.label}>Node — {getNodeUrl()}</Text>
        {chainInfo && (
          <Text style={styles.infoText}>
            Height: {chainInfo.height} | Supply: {(chainInfo.circulating_supply / 1e8).toFixed(0)} OPC
          </Text>
        )}
      </View>
      <View style={styles.actions}>
        <TouchableOpacity
          style={[styles.actionButton, styles.sendButton]}
          onPress={() => navigation.navigate('Send')}
        >
          <Text style={styles.actionButtonText}>Send</Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={[styles.actionButton, styles.receiveButton]}
          onPress={() => navigation.navigate('Receive')}
        >
          <Text style={styles.actionButtonText}>Receive</Text>
        </TouchableOpacity>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#0d1117' },
  centered: { flex: 1, justifyContent: 'center', alignItems: 'center', padding: 20 },
  loadingText: { color: '#8b949e', marginTop: 12, fontSize: 16 },
  card: { backgroundColor: '#161b22', margin: 12, padding: 20, borderRadius: 12, borderWidth: 1, borderColor: '#30363d' },
  balanceLabel: { color: '#8b949e', fontSize: 14, marginBottom: 4 },
  balanceAmount: { color: '#f7931a', fontSize: 36, fontWeight: '700' },
  lockedText: { color: '#8b949e', fontSize: 13, marginTop: 4 },
  label: { color: '#8b949e', fontSize: 14, marginBottom: 4 },
  address: { color: '#58a6ff', fontSize: 14, fontFamily: 'monospace' },
  infoText: { color: '#c9d1d9', fontSize: 14 },
  actions: { flexDirection: 'row', justifyContent: 'space-around', margin: 12 },
  actionButton: { flex: 1, padding: 16, borderRadius: 12, marginHorizontal: 6, alignItems: 'center' },
  sendButton: { backgroundColor: '#f7931a' },
  receiveButton: { backgroundColor: '#238636' },
  actionButtonText: { color: '#fff', fontSize: 18, fontWeight: '600' },
  button: { backgroundColor: '#f7931a', padding: 14, borderRadius: 8, marginTop: 8 },
  buttonText: { color: '#fff', fontSize: 16, fontWeight: '600' },
});
