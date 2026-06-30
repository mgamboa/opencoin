import React, { useState, useEffect } from 'react';
import { StatusBar } from 'expo-status-bar';
import { View, Text, TextInput, TouchableOpacity, ActivityIndicator, StyleSheet, Alert } from 'react-native';
import { NavigationContainer } from '@react-navigation/native';
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs';
import DashboardScreen from './src/screens/DashboardScreen';
import SendScreen from './src/screens/SendScreen';
import ReceiveScreen from './src/screens/ReceiveScreen';
import SettingsScreen from './src/screens/SettingsScreen';
import { discoverNode } from './src/services/rpc';
import { hasWallet, createWallet, importWallet } from './src/services/localwallet';

const Tab = createBottomTabNavigator();

function TabIcon({ label, focused }) {
  const icons = {
    Dashboard: focused ? '⬡' : '◇',
    Send: focused ? '↑' : '⇧',
    Receive: focused ? '↓' : '⇩',
    Settings: focused ? '⚙' : '⚬',
  };
  return <Text style={{ fontSize: 22, color: focused ? '#f7931a' : '#484f58' }}>{icons[label]}</Text>;
}

function SplashScreen() {
  return (
    <View style={splash.container}>
      <ActivityIndicator size="large" color="#f7931a" />
      <Text style={splash.text}>Discovering node...</Text>
    </View>
  );
}

function OnboardingScreen({ onDone }) {
  const [mode, setMode] = useState(null);
  const [importHex, setImportHex] = useState('');
  const [busy, setBusy] = useState(false);

  const handleCreate = async () => {
    setBusy(true);
    try {
      await createWallet();
      onDone();
    } catch (e) {
      Alert.alert('Error', e.message);
    }
    setBusy(false);
  };

  const handleImport = async () => {
    if (!importHex.trim() || importHex.trim().length !== 64) {
      Alert.alert('Error', 'Enter a valid 64-hex-char secret key');
      return;
    }
    setBusy(true);
    try {
      await importWallet(importHex.trim());
      onDone();
    } catch (e) {
      Alert.alert('Error', e.message);
    }
    setBusy(false);
  };

  return (
    <View style={splash.container}>
      <Text style={{ color: '#f7931a', fontSize: 28, fontWeight: '700', marginBottom: 8 }}>OpenCoin</Text>
      <Text style={{ color: '#8b949e', fontSize: 14, marginBottom: 32 }}>Mobile Wallet</Text>
      {!mode && (
        <>
          <TouchableOpacity style={styles.btn} onPress={handleCreate} disabled={busy}>
            <Text style={styles.btnText}>{busy ? 'Creating...' : 'Create New Wallet'}</Text>
          </TouchableOpacity>
          <TouchableOpacity style={[styles.btn, { backgroundColor: '#21262d', marginTop: 12 }]} onPress={() => setMode('import')}>
            <Text style={[styles.btnText, { color: '#c9d1d9' }]}>Import Existing Wallet</Text>
          </TouchableOpacity>
        </>
      )}
      {mode === 'import' && (
        <>
          <Text style={{ color: '#8b949e', fontSize: 13, marginBottom: 12 }}>Paste your 64-hex-char secret key</Text>
          <TextInput
            style={{ backgroundColor: '#161b22', color: '#c9d1d9', borderWidth: 1, borderColor: '#30363d', borderRadius: 8, padding: 14, fontSize: 14, fontFamily: 'monospace', width: '90%', marginBottom: 16 }}
            value={importHex}
            onChangeText={setImportHex}
            placeholder="c5af8cd9463d7caaf36dbfd58b609cc80caa1502d66ada61d2fbcb274c40ff64"
            placeholderTextColor="#484f58"
            autoCapitalize="none"
            autoCorrect={false}
          />
          <TouchableOpacity style={styles.btn} onPress={handleImport} disabled={busy}>
            <Text style={styles.btnText}>{busy ? 'Importing...' : 'Import Wallet'}</Text>
          </TouchableOpacity>
          <TouchableOpacity style={{ marginTop: 16 }} onPress={() => setMode(null)}>
            <Text style={{ color: '#58a6ff', fontSize: 14 }}>← Back</Text>
          </TouchableOpacity>
        </>
      )}
    </View>
  );
}

export default function App() {
  const [ready, setReady] = useState(false);
  const [hasWalletReady, setHasWalletReady] = useState(false);
  const [walletExists, setWalletExists] = useState(false);
  const [splashError, setSplashError] = useState(null);

  useEffect(() => {
    (async () => {
      try {
        await discoverNode();
      } catch (e) {
        setSplashError(e.message);
      }
      const exists = await hasWallet();
      setWalletExists(exists);
      setHasWalletReady(true);
      setReady(true);
    })();
  }, []);

  if (!ready) {
    return (
      <>
        <StatusBar style="light" />
        <SplashScreen />
      </>
    );
  }

  if (!walletExists) {
    return (
      <>
        <StatusBar style="light" />
        <OnboardingScreen onDone={() => setWalletExists(true)} />
      </>
    );
  }

  return (
    <>
      <StatusBar style="light" />
      <NavigationContainer>
        <Tab.Navigator
          screenOptions={({ route }) => ({
            tabBarIcon: ({ focused }) => <TabIcon label={route.name} focused={focused} />,
            tabBarActiveTintColor: '#f7931a',
            tabBarInactiveTintColor: '#484f58',
            tabBarStyle: { backgroundColor: '#161b22', borderTopColor: '#30363d' },
            headerStyle: { backgroundColor: '#0d1117' },
            headerTintColor: '#c9d1d9',
          })}
        >
          <Tab.Screen name="Dashboard" component={DashboardScreen} />
          <Tab.Screen name="Send" component={SendScreen} />
          <Tab.Screen name="Receive" component={ReceiveScreen} />
          <Tab.Screen name="Settings" component={SettingsScreen} />
        </Tab.Navigator>
      </NavigationContainer>
    </>
  );
}

const splash = StyleSheet.create({
  container: { flex: 1, justifyContent: 'center', alignItems: 'center', backgroundColor: '#0d1117', padding: 20 },
  text: { color: '#8b949e', marginTop: 12, fontSize: 16 },
});

const styles = StyleSheet.create({
  btn: { backgroundColor: '#f7931a', padding: 16, borderRadius: 12, alignItems: 'center', width: '80%' },
  btnText: { color: '#fff', fontSize: 16, fontWeight: '600' },
});
