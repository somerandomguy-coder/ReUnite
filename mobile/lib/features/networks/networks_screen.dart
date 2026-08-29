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

  /// What the platform has actually told us, rendered as one line each.
  ///
  /// This panel exists because "no peers yet" and "the radio never started" look
  /// identical from the outside and are completely different problems. Every row states
  /// something the app was told; none of them guesses.
  @override
  Widget build(BuildContext context) {
    final bluetooth = mesh.usingBluetooth;
    final state = mesh.radioState;
    final (Color colour, IconData icon, String headline) = switch (state) {
      'on' => (Colors.cyanAccent, Icons.bluetooth_connected, 'Bluetooth radio is on'),
      'off' => (Colors.orangeAccent, Icons.bluetooth_disabled, 'Bluetooth is switched off'),
      'unauthorized' => (
          Colors.orangeAccent,
          Icons.lock_outline,
          'Bluetooth permission was refused'
        ),
      'unsupported' => (Colors.grey, Icons.block, 'No Bluetooth LE radio on this device'),
      'resetting' => (Colors.grey, Icons.sync, 'Bluetooth is restarting'),
      _ => (Colors.grey, Icons.hourglass_empty, 'Waiting for the radio to report in'),
    };

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const Text('Radio', style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 6),
        Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: Colors.blue.shade900.withValues(alpha: 0.2),
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: colour.withValues(alpha: 0.5)),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(children: [
                Icon(bluetooth ? icon : Icons.wifi, color: colour),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    bluetooth ? headline : 'Meshing over ${mesh.radioNames}',
                    style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 13),
                  ),
                ),
              ]),
              const SizedBox(height: 8),
              if (mesh.radioNotice != null)
                _DiagLine(label: 'Note', value: mesh.radioNotice!, warn: true),
              _DiagLine(label: 'Radios', value: mesh.radioNames),
              if (bluetooth) ...[
                _DiagLine(label: 'Bluetooth state', value: state),
                _DiagLine(
                  label: 'Connected peers',
                  value: '${mesh.bleConnected}',
                  warn: mesh.bleConnected == 0 && state == 'on',
                ),
                if (mesh.bleError != null)
                  _DiagLine(label: 'Last error', value: mesh.bleError!, warn: true),
                const SizedBox(height: 6),
                Text(
                  mesh.bleConnected == 0 && state == 'on'
                      ? 'The radio is running and has found nobody yet. Both phones need '
                          'the app open and on screen, within a few metres for the first '
                          'connection.'
                      : 'Frames cross as opaque bytes; every protocol decision stays in '
                          'the mesh core.',
                  style: const TextStyle(fontSize: 11, color: Colors.grey),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

/// One fact, labelled. Deliberately dull: this panel is read when something is wrong.
class _DiagLine extends StatelessWidget {
  final String label;
  final String value;
  final bool warn;
  const _DiagLine({required this.label, required this.value, this.warn = false});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        SizedBox(
          width: 120,
          child: Text(label,
              style: const TextStyle(fontSize: 11, color: Colors.grey)),
        ),
        Expanded(
          child: SelectableText(
            value,
            style: TextStyle(
              fontSize: 11,
              fontFamily: 'monospace',
              color: warn ? Colors.orangeAccent : Colors.white70,
            ),
          ),
        ),
      ]),
    );
  }
}
