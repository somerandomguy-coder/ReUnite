import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../../models/mesh_models.dart';
import '../../services/mesh_service.dart';

/// Private networks: create, invite, switch, storing, kick.
///
/// A private network is a symmetric key that only ever leaves this device sealed to one
/// invited member's public key. Relays carry the traffic without being able to read it.
class NetworksScreen extends StatelessWidget {
  const NetworksScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final mesh = context.watch<MeshService>();
    return Scaffold(
      appBar: AppBar(
        title: const Text('Networks'),
        actions: [
          IconButton(
            tooltip: 'Create a private network',
            icon: const Icon(Icons.add),
            onPressed: () => _promptCreate(context, mesh),
          ),
        ],
      ),
      body: ListView(
        children: [
          _MyNode(mesh: mesh),
          const Divider(),
          ...mesh.networks.map((n) => _NetworkTile(network: n, mesh: mesh)),
          const SizedBox(height: 24),
        ],
      ),
    );
  }

  void _promptCreate(BuildContext context, MeshService mesh) {
    final controller = TextEditingController();
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: const Text('New private network'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(hintText: 'e.g. rescue'),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(dialogContext), child: const Text('Cancel')),
          FilledButton(
            onPressed: () {
              final err = mesh.createNetwork(controller.text.trim());
              Navigator.pop(dialogContext);
              if (err != null && context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(err)));
              }
            },
            child: const Text('Create'),
          ),
        ],
      ),
    );
  }
}

class _MyNode extends StatelessWidget {
  final MeshService mesh;
  const _MyNode({required this.mesh});

  @override
  Widget build(BuildContext context) {
    final me = mesh.me;
    return Padding(
      padding: const EdgeInsets.all(16),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        const Text('This device', style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 6),
        SelectableText(mesh.nodeId, style: const TextStyle(fontFamily: 'monospace')),
        const SizedBox(height: 4),
        Text(
          'Share this id with someone so they can invite you to their network.',
          style: TextStyle(fontSize: 11, color: Colors.grey.shade500),
        ),
        const SizedBox(height: 8),
        Text('transport: ${me?.transport ?? '-'}',
            style: const TextStyle(fontSize: 11, color: Colors.grey)),
        if (me?.battery != null)
          Text('battery: ${me!.battery}%',
              style: const TextStyle(fontSize: 11, color: Colors.grey)),
        const SizedBox(height: 14),
        _TransportPicker(mesh: mesh),
      ]),
    );
  }
}

class _NetworkTile extends StatelessWidget {
  final NetworkInfo network;
  final MeshService mesh;
  const _NetworkTile({required this.network, required this.mesh});

  @override
  Widget build(BuildContext context) {
    return Card(
      color: network.active ? const Color(0xFF243027) : const Color(0xFF23282E),
      margin: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      child: Column(children: [
        ListTile(
          leading: Icon(
            network.isDefault ? Icons.public : Icons.lock,
            color: network.isDefault ? Colors.grey : Colors.tealAccent,
          ),
          title: Row(children: [
            Text(network.name, style: const TextStyle(fontWeight: FontWeight.bold)),
            if (network.active) ...[
              const SizedBox(width: 8),
              const Chip(
                label: Text('active', style: TextStyle(fontSize: 10)),
                visualDensity: VisualDensity.compact,
                padding: EdgeInsets.zero,
              ),
            ],
          ]),
          subtitle: Text(
            network.isDefault
                ? 'Public lobby — everyone in range. Treat anything here as public.'
                : '${network.memberCount} member(s) · epoch ${network.epoch}'
                    ' · storing ${network.storeMessages ? "on" : "off"}',
            style: const TextStyle(fontSize: 11),
          ),
          trailing: network.active
              ? null
              : TextButton(
                  onPressed: () => mesh.switchNetwork(network.name),
                  child: const Text('Switch'),
                ),
        ),
        if (!network.isDefault)
          OverflowBar(
            alignment: MainAxisAlignment.end,
            children: [
              TextButton.icon(
                icon: const Icon(Icons.person_add, size: 18),
                label: const Text('Invite'),
                onPressed: () => _promptUser(
                  context,
                  title: 'Invite to ${network.name}',
                  hint: 'node id, id prefix, or a name you set',
                  onSubmit: (v) => mesh.invite(network.name, v),
                ),
              ),
              TextButton.icon(
                icon: Icon(network.storeMessages ? Icons.save : Icons.save_outlined, size: 18),
                label: Text(network.storeMessages ? 'Storing on' : 'Storing off'),
                onPressed: () => mesh.setStoring(network.name, !network.storeMessages),
              ),
              TextButton.icon(
                icon: const Icon(Icons.how_to_vote, size: 18),
                label: const Text('Kick'),
                onPressed: () => _promptUser(
                  context,
                  title: 'Vote to remove from ${network.name}',
                  hint: 'needs 50% of members to agree',
                  onSubmit: (v) => mesh.kick(v),
                ),
              ),
            ],
          ),
        if (!network.isDefault && network.members.isNotEmpty)
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Text('members: ${network.members.join(", ")}',
                  style: const TextStyle(fontSize: 11, color: Colors.grey)),
            ),
          ),
      ]),
    );
  }

  void _promptUser(BuildContext context,
      {required String title, required String hint, required String? Function(String) onSubmit}) {
    final controller = TextEditingController();
    showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(title),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: InputDecoration(hintText: hint),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(dialogContext), child: const Text('Cancel')),
          FilledButton(
            onPressed: () {
              final err = onSubmit(controller.text.trim());
              Navigator.pop(dialogContext);
              if (err != null && context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(err)));
              }
            },
            child: const Text('OK'),
          ),
        ],
      ),
    );
  }
}

/// Choose the radio.
///
/// Wi-Fi reaches laptops and works over any shared network, including a hotspot with no
/// internet. Bluetooth needs no infrastructure whatsoever, which is the case that matters
/// when there is nothing left standing - but it is phone-to-phone only, because laptops
/// cannot advertise as BLE peripherals from userspace.
class _TransportPicker extends StatelessWidget {
  final MeshService mesh;
  const _TransportPicker({required this.mesh});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('Radio', style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 6),
        SegmentedButton<MeshTransport>(
          segments: [
            const ButtonSegment(
              value: MeshTransport.wifi,
              icon: Icon(Icons.wifi, size: 18),
              label: Text('Wi-Fi'),
            ),
            ButtonSegment(
              value: MeshTransport.bluetooth,
              icon: const Icon(Icons.bluetooth, size: 18),
              label: const Text('Bluetooth'),
              enabled: mesh.bluetoothAvailable,
            ),
          ],
          selected: {mesh.transport},
          onSelectionChanged: (selection) async {
            final to = selection.first;
            if (to == mesh.transport) return;
            final messenger = ScaffoldMessenger.of(context);
            messenger.showSnackBar(SnackBar(
              content: Text('switching to ${to == MeshTransport.bluetooth ? 'Bluetooth' : 'Wi-Fi'}...'),
              duration: const Duration(seconds: 1),
            ));
            await mesh.switchTransport(to);
          },
        ),
        const SizedBox(height: 6),
        if (!mesh.bluetoothAvailable)
          const Text(
            'Bluetooth mesh runs on phones. A laptop cannot advertise as a BLE peripheral, '
            'so desktops stay on Wi-Fi.',
            style: TextStyle(fontSize: 11, color: Colors.grey),
          )
        else if (mesh.transport == MeshTransport.bluetooth) ...[
          if (mesh.bleError != null)
            Row(children: [
              const Icon(Icons.error_outline, size: 14, color: Colors.orangeAccent),
              const SizedBox(width: 6),
              Expanded(
                child: Text(mesh.bleError!,
                    style: const TextStyle(fontSize: 11, color: Colors.orangeAccent)),
              ),
            ])
          else
            Text(
              mesh.bleConnected == 0
                  ? 'Searching for other phones... keep the app open on both.'
                  : 'Connected to ${mesh.bleConnected} phone(s) over Bluetooth. No Wi-Fi needed.',
              style: TextStyle(
                fontSize: 11,
                color: mesh.bleConnected == 0 ? Colors.grey : Colors.tealAccent,
              ),
            ),
        ] else
          const Text('Every device must be on the same Wi-Fi or hotspot.',
              style: TextStyle(fontSize: 11, color: Colors.grey)),
      ],
    );
  }
}
