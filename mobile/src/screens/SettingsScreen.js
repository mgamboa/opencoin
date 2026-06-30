import React, { useState } from 'react';
import { View, Text, TextInput, TouchableOpacity, StyleSheet, Alert } from 'react-native';
import { getNodeUrl, setNodeUrl } from '../services/rpc';

export default function SettingsScreen() {
  const [url, setUrl] = useState(getNodeUrl());

  const handleSave = () => {
    let cleanUrl = url.trim();
    if (!cleanUrl.startsWith('http://') && !cleanUrl.startsWith('https://')) {
      cleanUrl = 'http://' + cleanUrl;
    }
    setNodeUrl(cleanUrl);
    setUrl(cleanUrl);
    Alert.alert('Saved', `Node URL set to ${cleanUrl}`);
  };

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Settings</Text>
      <Text style={styles.label}>Node RPC URL</Text>
      <TextInput
        style={styles.input}
        value={url}
        onChangeText={setUrl}
        placeholder="http://192.168.2.10:8545"
        placeholderTextColor="#484f58"
        autoCapitalize="none"
        autoCorrect={false}
      />
      <TouchableOpacity style={styles.button} onPress={handleSave}>
        <Text style={styles.buttonText}>Save</Text>
      </TouchableOpacity>
      <Text style={styles.version}>OpenCoin Mobile v0.1.0</Text>
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
  version: { color: '#484f58', fontSize: 12, textAlign: 'center', marginTop: 40, position: 'absolute', bottom: 30, left: 0, right: 0 },
});
