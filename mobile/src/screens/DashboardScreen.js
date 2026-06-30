import React, { useState, useEffect, useCallback } from 'react';
import {
  View, Text, StyleSheet, TouchableOpacity, RefreshControl, ScrollView,
  ActivityIndicator,
} from 'react-native';
import { fetchWalletData } from '../services/wallet';
import { getNodeUrl } from '../services/rpc';

export default function DashboardScreen({ navigation }) {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState(null);

  const load = useCallback(async () => {
    try {
      const result = await fetchWalletData();
      if (result.error) {
        setError(result.error);
        setData(null);
      } else {
        setData(result);
        setError(null);
      }
    } catch (e) {
      setError(e.message);
    }
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
        <Text style={styles.loadingText}>Connecting to node...</Text>
      </View>
    );
  }

  return (
    <ScrollView
      style={styles.container}
      refreshControl={<RefreshControl refreshing={refreshing} onRefresh={onRefresh} />}
    >
      {error ? (
        <View style={styles.centered}>
          <Text style={styles.errorText}>{error}</Text>
          <TouchableOpacity style={styles.button} onPress={load}>
            <Text style={styles.buttonText}>Retry</Text>
          </TouchableOpacity>
        </View>
      ) : data ? (
        <>
          <View style={styles.card}>
            <Text style={styles.balanceLabel}>Balance</Text>
            <Text style={styles.balanceAmount}>
              {(data.balance.balance / 1e8).toFixed(4)} OPC
            </Text>
            {data.balance.locked > 0 && (
              <Text style={styles.lockedText}>
                Locked: {(data.balance.locked / 1e8).toFixed(4)} OPC
              </Text>
            )}
          </View>
          <View style={styles.card}>
            <Text style={styles.label}>Address</Text>
            <Text style={styles.address} numberOfLines={2}>
              {data.balance.address}
            </Text>
          </View>
          <View style={styles.card}>
            <Text style={styles.label}>Node — {getNodeUrl()}</Text>
            <Text style={styles.infoText}>
              Height: {data.info.height} | Supply: {(data.info.circulating_supply / 1e8).toFixed(0)} OPC
            </Text>
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
        </>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#0d1117' },
  centered: { flex: 1, justifyContent: 'center', alignItems: 'center', padding: 20 },
  loadingText: { color: '#8b949e', marginTop: 12, fontSize: 16 },
  errorText: { color: '#f85149', fontSize: 16, textAlign: 'center', marginBottom: 16 },
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
