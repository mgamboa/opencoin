import React from 'react';
import { StatusBar } from 'expo-status-bar';
import { NavigationContainer } from '@react-navigation/native';
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs';
import { Text } from 'react-native';
import DashboardScreen from './src/screens/DashboardScreen';
import SendScreen from './src/screens/SendScreen';
import ReceiveScreen from './src/screens/ReceiveScreen';
import SettingsScreen from './src/screens/SettingsScreen';

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

export default function App() {
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
