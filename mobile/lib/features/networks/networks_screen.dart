import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../services/mesh_service.dart';

class NetworksScreen extends StatelessWidget {
  const NetworksScreen({super.key});

  Future<void> _showCreateNetworkDialog(BuildContext context) async {
    final controller = TextEditingController();
    final meshService = Provider.of<MeshService>(context, listen: false);

    return showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: const Color(0xFF1E1E1E),
        title: const Text('Create Private Network'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(hintText: 'Network name'),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () {
              final name = controller.text.trim();
              if (name.isNotEmpty) {
                meshService.createNetwork(name);
              }
              Navigator.of(dialogContext).pop();
            },
            child: const Text('Create', style: TextStyle(color: Colors.amber)),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final meshService = Provider.of<MeshService>(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Networks'),
        actions: [
          IconButton(
            icon: const Icon(Icons.add),
            onPressed: () => _showCreateNetworkDialog(context),
          ),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.all(12),
        children: [
          Card(
            color: const Color(0xFF1E1E1E),
            child: ListTile(
              leading: const Icon(Icons.security, color: Colors.amber),
              title: Text(meshService.activeNetwork),
              subtitle: Text('Node ID: ${meshService.nodeId}'),
              trailing: const Chip(
                label: Text('active'),
                backgroundColor: Colors.amber,
                labelStyle: TextStyle(color: Colors.black),
              ),
            ),
          ),
          const Padding(
            padding: EdgeInsets.symmetric(vertical: 16.0),
            child: Text(
              'Tap + to create a new private network and switch to it.',
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.grey),
            ),
          ),
        ],
      ),
    );
  }
}
