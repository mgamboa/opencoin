import React, { useState } from 'react';
import {
  View, Text, TextInput, TouchableOpacity, StyleSheet, Alert, ScrollView,
  ActivityIndicator,
} from 'react-native';
import { sendFromWallet } from '../services/localwallet';

export default function SendScreen({ navigation }) {
  const [address, setAddress] = useState('');
  const [amount, setAmount] = useState('');
  const [fee, setFee] = useState('0.00000010');
  const [sending, setSending] = useState(false);
  const [result, setResult] = useState(null);

  const handleSend = async () => {
    if (!address.trim()) {
      Alert.alert('Error', 'Enter a recipient address');
      return;
    }
    const amountInt = Math.round(parseFloat(amount) * 1e8);
    if (isNaN(amountInt) || amountInt <= 0) {
      Alert.alert('Error', 'Enter a valid amount');
      return;
    }
    const feeInt = Math.round(parseFloat(fee || '0.00000010') * 1e8);
    setSending(true);
    setResult(null);
    const res = await sendFromWallet(address.trim(), amountInt, feeInt);
    setSending(false);
    if (res.success) {
      setResult({ success: true, txHash: res.txHash });
    } else {
      setResult({ success: false, error: res.error });
    }
  };

  return (
    <ScrollView style={styles.container} keyboardShouldPersistTaps="handled">
      <Text style={styles.label}>Recipient Address</Text>
      <TextInput
        style={styles.input}
        value={address}
        onChangeText={setAddress}
        placeholder="OC..."
        placeholderTextColor="#484f58"
        autoCapitalize="none"
        autoCorrect={false}
      />
      <Text style={styles.label}>Amount (OPC)</Text>
      <TextInput
        style={styles.input}
        value={amount}
        onChangeText={setAmount}
        placeholder="0.0000"
        placeholderTextColor="#484f58"
        keyboardType="decimal-pad"
      />
      <Text style={styles.label}>Fee (OPC)</Text>
      <TextInput
        style={styles.input}
        value={fee}
        onChangeText={setFee}
        placeholder="0.00000010"
        placeholderTextColor="#484f58"
        keyboardType="decimal-pad"
      />
      <TouchableOpacity
        style={[styles.button, sending && styles.buttonDisabled]}
        onPress={handleSend}
        disabled={sending}
      >
        {sending ? (
          <ActivityIndicator color="#fff" />
        ) : (
          <Text style={styles.buttonText}>Send</Text>
        )}
      </TouchableOpacity>
      {result && (
        <View style={[styles.resultCard, result.success ? styles.successCard : styles.errorCard]}>
          {result.success ? (
            <>
              <Text style={styles.resultTitle}>Transaction Sent!</Text>
              <Text style={styles.resultTx}>{result.txHash}</Text>
            </>
          ) : (
            <>
              <Text style={styles.resultTitle}>Error</Text>
              <Text style={styles.resultError}>{result.error}</Text>
            </>
          )}
        </View>
      )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#0d1117', padding: 16 },
  label: { color: '#c9d1d9', fontSize: 14, marginBottom: 6, marginTop: 12 },
  input: {
    backgroundColor: '#161b22', color: '#c9d1d9', borderWidth: 1, borderColor: '#30363d',
    borderRadius: 8, padding: 14, fontSize: 16, fontFamily: 'monospace',
  },
  button: {
    backgroundColor: '#f7931a', padding: 16, borderRadius: 12, alignItems: 'center',
    marginTop: 24,
  },
  buttonDisabled: { opacity: 0.6 },
  buttonText: { color: '#fff', fontSize: 18, fontWeight: '600' },
  resultCard: { padding: 16, borderRadius: 8, marginTop: 16 },
  successCard: { backgroundColor: '#161b22', borderWidth: 1, borderColor: '#238636' },
  errorCard: { backgroundColor: '#161b22', borderWidth: 1, borderColor: '#f85149' },
  resultTitle: { color: '#c9d1d9', fontSize: 16, fontWeight: '600', marginBottom: 8 },
  resultTx: { color: '#58a6ff', fontSize: 12, fontFamily: 'monospace' },
  resultError: { color: '#f85149', fontSize: 14 },
});
