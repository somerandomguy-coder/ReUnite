import 'package:flutter/material.dart';
import 'shared/theme.dart';
import 'features/chat/chat_screen.dart';
import 'features/map/map_screen.dart';
import 'features/networks/networks_screen.dart';

class ReUniteApp extends StatelessWidget {
  const ReUniteApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ReUnite Emergency Mesh',
      debugShowCheckedModeBanner: false,
      theme: ReUniteTheme.darkTheme,
      home: const MainNavigationScreen(),
    );
  }
}

class MainNavigationScreen extends StatefulWidget {
  const MainNavigationScreen({super.key});

  @override
  State<MainNavigationScreen> createState() => _MainNavigationScreenState();
}

class _MainNavigationScreenState extends State<MainNavigationScreen> {
  int _currentIndex = 0;

  final List<Widget> _screens = const [
    ChatScreen(),
    MapScreen(),
    NetworksScreen(),
  ];

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: _screens[_currentIndex],
      bottomNavigationBar: BottomNavigationBar(
        currentIndex: _currentIndex,
        selectedItemColor: Colors.amber,
        unselectedItemColor: Colors.grey,
        backgroundColor: const Color(0xFF1E1E1E),
        onTap: (index) => setState(() => _currentIndex = index),
        items: const [
          BottomNavigationBarItem(
            icon: Icon(Icons.chat_bubble_outline),
            activeIcon: Icon(Icons.chat_bubble),
            label: 'Emergency Chat',
          ),
          BottomNavigationBarItem(
            icon: Icon(Icons.map_outlined),
            activeIcon: Icon(Icons.map),
            label: 'GPS & Peers',
          ),
          BottomNavigationBarItem(
            icon: Icon(Icons.security_outlined),
            activeIcon: Icon(Icons.security),
            label: 'Networks',
          ),
        ],
      ),
    );
  }
}
