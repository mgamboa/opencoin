import React, { useState } from 'react';
import { View, Text, TextInput, TouchableOpacity, StyleSheet, Alert } from 'react-native';
import { getNodeUrl, setNodeUrl } from '../services/rpc';
import { loadWallet, clearWallet } from '../services/localwallet';

export default function SettingsScreen() {
  const [url, setUrl] = useState(getNodeUrl());
  const [showSecret, setShowSecret] = useState(false);
  const [secret, setSecret] = useState('');

  const handleSave = () => {
    let cleanUrl = url.trim();
    if (!cleanUrl.startsWith('http://') && !cleanUrl.startsWith('https://')) {
      cleanUrl = 'http://' + cleanUrl;
    }
    setNodeUrl(cleanUrl);
    setUrl(cleanUrl);
    Alert.alert('Saved', `Node URL set to ${cleanUrl}`);
  };

  const handleShowSecret = async () => {
    const w = await loadWallet();
    if (w) {
      setSecret(w.secretHex);
      setShowSecret(true);
    }
  };

  const handleClearWallet = () => {
    Alert.alert(
      'Clear Wallet',
      'Are you sure? This will delete your secret key from this device. Make sure you have backed it up!',
      [
        { text: 'Cancel', style: 'cancel' },
        { text: 'Clear', style: 'destructive', onPress: async () => {
          await clearWallet();
          setShowSecret(false);
          setSecret('');
          Alert.alert('Cleared', 'Wallet deleted from this device. Restart the app to create/import a new one.');
        }},
      ]
    );
  };

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Settings</Text>
      <Text style={styles.label}>Node RPC URL</Text>
      <TextInput
        style={styles.input}
        value={url}
        onChangeText={setUrl}
        placeholder="http://192.168.2.10:9769"
        placeholderTextColor="#484f58"
        autoCapitalize="none"
        autoCorrect={false}
      />
      <TouchableOpacity style={styles.button} onPress={handleSave}>
        <Text style={styles.buttonText}>Save</Text>
      </TouchableOpacity>

      <Text style={[styles.label, { marginTop: 24 }]}>Wallet</Text>
      <TouchableOpacity style={styles.secButton} onPress={handleShowSecret}>
        <Text style={styles.secButtonText}>Show Secret Key</Text>
      </TouchableOpacity>
      {showSecret && (
        <Text style={styles.secretBox} selectable>{secret}</Text>
      )}
      <TouchableOpacity style={[styles.secButton, { borderColor: '#f85149', marginTop: 12 }]} onPress={handleClearWallet}>
        <Text style={[styles.secButtonText, { color: '#f85149' }]}>Clear Wallet</Text>
      </TouchableOpacity>

      <Text style={styles.version}>OpenCoin Mobile v0.2.0</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#0d1117', padding: 16 },
  title: { color: '#c9d1d9', fontSize: 22, fontWeight: '600', marginBottom: 24, marginTop: 16 },
  label: { color: '#c9d1d9', fontSize: 14, marginBottom: 6 },
  input: {
    backgroundColor: '#161b22', color: '#c9d1d9', borderWidth: 1, borderColor: '#30363d',
    borderRadius: 8, padding: 14, fontSize: 16, fontFamily: 'monospace',
  },
  button: { backgroundColor: '#f7931a', padding: 16, borderRadius: 12, alignItems: 'center', marginTop: 24 },
  buttonText: { color: '#fff', fontSize: 18, fontWeight: '600' },
  secButton: { backgroundColor: '#161b22', padding: 16, borderRadius: 12, alignItems: 'center', marginTop: 8, borderWidth: 1, borderColor: '#30363d' },
  secButtonText: { color: '#58a6ff', fontSize: 16, fontWeight: '600' },
  secretBox: { backgroundColor: '#161b22', color: '#f7931a', padding: 12, borderRadius: 8, marginTop: 8, fontFamily: 'monospace', fontSize: 12, borderWidth: 1, borderColor: '#30363d' },
  version: { color: '#484f58', fontSize: 12, textAlign: 'center', marginTop: 40, position: 'absolute', bottom: 30, left: 0, right: 0 },
});
