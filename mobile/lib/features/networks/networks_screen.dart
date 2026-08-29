import 'package:flutter/material.dart';

class NetworksScreen extends StatelessWidget {
  const NetworksScreen({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Encrypted Private Networks'),
      ),
      body: const Center(
        child: Text(
          'Private Networks Feature\n(Reserved for Developer 2)',
          textAlign: TextAlign.center,
          style: TextStyle(color: Colors.grey),
        ),
      ),
    );
  }
}
