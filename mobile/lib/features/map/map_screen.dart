import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../services/mesh_service.dart';

class MapScreen extends StatelessWidget {
  const MapScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final meshService = Provider.of<MeshService>(context);

    return Scaffold(
      appBar: AppBar(
        title: const Text('GPS & Nearby Peer Radar'),
      ),
      body: Column(
        children: [
          Container(
            height: 200,
            color: const Color(0xFF1A2634),
            child: const Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(Icons.radar, size: 64, color: Colors.cyan),
                  SizedBox(height: 8),
                  Text(
                    'P2P GPS Mesh Radar',
                    style: TextStyle(fontWeight: FontWeight.bold, fontSize: 16),
                  ),
                  Text(
                    'Tracking nearby nodes over Bluetooth & Wi-Fi Direct',
                    style: TextStyle(color: Colors.grey, fontSize: 12),
                  ),
                ],
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(12.0),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const Text(
                  'Reachable Nodes (Nearest First)',
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                Text(
                  '${meshService.peers.length} active',
                  style: const TextStyle(color: Colors.amber),
                ),
              ],
            ),
          ),
          Expanded(
            child: ListView.builder(
              itemCount: meshService.peers.length,
              itemBuilder: (context, index) {
                final peer = meshService.peers[index];
                return ListTile(
                  leading: const CircleAvatar(
                    backgroundColor: Colors.cyan,
                    child: Icon(Icons.person, color: Colors.black),
                  ),
                  title: Text(peer.name),
                  subtitle: Text('ID: ${peer.id} • ${peer.hops} hop(s)'),
                  trailing: Text(
                    peer.distanceMeters != null
                        ? '${peer.distanceMeters!.toStringAsFixed(0)}m away'
                        : 'In range',
                    style: const TextStyle(color: Colors.amber),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}
