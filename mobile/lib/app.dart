import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'services/mesh_service.dart';
import 'shared/theme.dart';
import 'features/chat/chat_screen.dart';
import 'features/map/map_screen.dart';
import 'features/emergency/emergency_screen.dart';
import 'features/networks/networks_screen.dart';

class ReUniteApp extends StatelessWidget {
  const ReUniteApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ReUnite Emergency Mesh',
      debugShowCheckedModeBanner: false,
      theme: ReUniteTheme.darkTheme,
      home: const _Gate(),
    );
  }
}

/// Zero-config onboarding: the node starts itself. The only thing that can stop it is a
/// missing native library, and that needs to say so loudly rather than showing an empty
/// screen someone will mistake for "no peers yet".
class _Gate extends StatelessWidget {
  const _Gate();

  @override
  Widget build(BuildContext context) {
    final mesh = context.watch<MeshService>();
    if (mesh.startError != null) return _StartupError(message: mesh.startError!);
    if (!mesh.started) {
      return const Scaffold(
        body: Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 16),
              Text('starting the mesh...'),
            ],
          ),
        ),
      );
    }
    return const MainNavigationScreen();
  }
}

class _StartupError extends StatelessWidget {
  final String message;
  const _StartupError({required this.message});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Row(children: [
              Icon(Icons.error_outline, color: Colors.redAccent),
              SizedBox(width: 8),
              Text('The mesh core did not start',
                  style: TextStyle(fontSize: 18, fontWeight: FontWeight.bold)),
            ]),
            const SizedBox(height: 16),
            SelectableText(message, style: const TextStyle(fontFamily: 'monospace', fontSize: 12)),
            const SizedBox(height: 24),
            const Text('Most likely the Rust core has not been built for this platform:'),
            const SizedBox(height: 8),
            const SelectableText('  ./scripts/build_ffi.sh macos\n'
                '  ./scripts/build_ffi.sh android',
                style: TextStyle(fontFamily: 'monospace', color: Colors.cyan)),
            const SizedBox(height: 16),
            const Text('See docs/MOBILE.md for the full setup.',
                style: TextStyle(color: Colors.grey)),
          ],
        ),
      ),
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

  static const _screens = [
    ChatScreen(),
    MapScreen(),
    EmergencyScreen(),
    NetworksScreen(),
  ];

  @override
  Widget build(BuildContext context) {
    final mesh = context.watch<MeshService>();
    final sosCount = mesh.sosPeers.length;
    return Scaffold(
      body: Column(
        children: [
          // An SOS anywhere on the mesh outranks whatever screen you are looking at.
          if (sosCount > 0 || mesh.sosActive) _SosBanner(mesh: mesh),
          Expanded(child: _screens[_currentIndex]),
        ],
      ),
      bottomNavigationBar: BottomNavigationBar(
        currentIndex: _currentIndex,
        type: BottomNavigationBarType.fixed,
        selectedItemColor: Colors.amber,
        unselectedItemColor: Colors.grey,
        backgroundColor: const Color(0xFF1E1E1E),
        onTap: (index) => setState(() => _currentIndex = index),
        items: [
          const BottomNavigationBarItem(
            icon: Icon(Icons.chat_bubble_outline),
            activeIcon: Icon(Icons.chat_bubble),
            label: 'Chat',
          ),
          const BottomNavigationBarItem(
            icon: Icon(Icons.explore_outlined),
            activeIcon: Icon(Icons.explore),
            label: 'Peers',
          ),
          BottomNavigationBarItem(
            icon: Badge(
              isLabelVisible: sosCount > 0,
              label: Text('$sosCount'),
              child: const Icon(Icons.emergency_outlined),
            ),
            activeIcon: const Icon(Icons.emergency),
            label: 'Emergency',
          ),
          const BottomNavigationBarItem(
            icon: Icon(Icons.security_outlined),
            activeIcon: Icon(Icons.security),
            label: 'Networks',
          ),
        ],
      ),
    );
  }
}

class _SosBanner extends StatelessWidget {
  final MeshService mesh;
  const _SosBanner({required this.mesh});

  @override
  Widget build(BuildContext context) {
    final others = mesh.sosPeers;
    final mine = mesh.sosActive;
    final text = mine
        ? 'YOUR SOS IS ACTIVE - the mesh has been alerted. Emergency services have NOT been called.'
        : '${others.length} SOS on the mesh: ${others.map((p) => p.display).join(', ')}';
    return Material(
      color: Colors.red.shade900,
      child: SafeArea(
        bottom: false,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          child: Row(
            children: [
              const Icon(Icons.warning_amber_rounded, color: Colors.white),
              const SizedBox(width: 10),
              Expanded(
                child: Text(text,
                    style: const TextStyle(color: Colors.white, fontWeight: FontWeight.w600)),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
