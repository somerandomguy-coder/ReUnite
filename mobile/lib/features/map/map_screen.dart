import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../../models/mesh_models.dart';
import '../../services/mesh_service.dart';
import 'widgets/radar.dart';

class MapScreen extends StatelessWidget {
  const MapScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final mesh = context.watch<MeshService>();
    final me = mesh.me;
    return Scaffold(
      appBar: AppBar(
        title: const Text('Peers'),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh),
            onPressed: mesh.refresh,
          ),
        ],
      ),
      body: ListView(
        children: [
          // No offline tiles are bundled, so Compass/Grid mode is the map. plan.md
          // §4 step 2.2 requires this degrade gracefully rather than showing an error.
          PeerRadar(peers: mesh.peers, myLat: me?.lat, myLon: me?.lon),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 16),
            child: Text(
              'Compass mode: bearing and distance from you. No offline map tiles are '
              'installed, so positions are shown relative rather than on a map.',
              style: TextStyle(color: Colors.grey, fontSize: 11),
            ),
          ),
          const Divider(height: 24),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                const Text('Reachable, nearest first',
                    style: TextStyle(fontWeight: FontWeight.bold)),
                Text(
                  '${mesh.livePeers.length} live · ${mesh.ghosts.length} ghost',
                  style: const TextStyle(color: Colors.amber, fontSize: 12),
                ),
              ],
            ),
          ),
          if (mesh.peers.isEmpty)
            const Padding(
              padding: EdgeInsets.all(28),
              child: Text(
                'No peers heard yet.\n\nCheck every device is on the same Wi-Fi, or add one '
                'by IP address from the Networks tab.',
                textAlign: TextAlign.center,
                style: TextStyle(color: Colors.grey),
              ),
            ),
          ...mesh.peers.map((p) => _PeerTile(peer: p, mesh: mesh)),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}

class _PeerTile extends StatelessWidget {
  final Peer peer;
  final MeshService mesh;
  const _PeerTile({required this.peer, required this.mesh});

  @override
  Widget build(BuildContext context) {
    final subtitle = <String>[
      peer.ghost ? 'unreachable' : (peer.direct ? 'direct' : 'relayed'),
      if (peer.hops != null) '${peer.hops} hop${peer.hops == 1 ? '' : 's'}',
      if (peer.rttMs != null) '${peer.rttMs}ms',
      if (peer.rssi != null) '${peer.rssi}dBm',
      formatAge(DateTime.now().millisecondsSinceEpoch - peer.lastSeenMs),
    ].join(' · ');

    return Opacity(
      // A ghost is dimmed, not deleted: where someone was last seen is the whole point.
      opacity: peer.ghost ? 0.55 : 1,
      child: ListTile(
        leading: CircleAvatar(
          backgroundColor: peer.sos
              ? Colors.redAccent
              : peer.ghost
                  ? Colors.grey.shade700
                  : Colors.cyan,
          child: Icon(
            peer.sos
                ? Icons.warning_amber_rounded
                : peer.ghost
                    ? Icons.person_off
                    : Icons.person,
            color: Colors.black,
          ),
        ),
        title: Row(children: [
          Flexible(child: Text(peer.display, overflow: TextOverflow.ellipsis)),
          if (peer.battery != null) ...[
            const SizedBox(width: 8),
            Icon(
              peer.battery! <= 15 ? Icons.battery_alert : Icons.battery_full,
              size: 14,
              color: peer.battery! <= 15 ? Colors.redAccent : Colors.grey,
            ),
            Text('${peer.battery}%', style: const TextStyle(fontSize: 11, color: Colors.grey)),
          ],
        ]),
        subtitle: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('${peer.id.substring(0, 8)}… · $subtitle',
                style: const TextStyle(fontSize: 11)),
            if (peer.sos)
              const Text('SOS — mesh alert only',
                  style: TextStyle(color: Colors.redAccent, fontWeight: FontWeight.bold, fontSize: 12)),
            if (peer.status != null)
              Text(mesh.describeStatus(peer.status!),
                  style: const TextStyle(color: Colors.amber, fontSize: 12)),
            if (peer.ghost && peer.hasPosition)
              Text(
                'last seen at ${peer.lat!.toStringAsFixed(5)}, ${peer.lon!.toStringAsFixed(5)}',
                style: const TextStyle(fontSize: 11, color: Colors.grey),
              ),
          ],
        ),
        trailing: Text(
          peer.distanceM != null ? formatDistance(peer.distanceM!) : '—',
          style: TextStyle(color: peer.ghost ? Colors.grey : Colors.amber),
        ),
        onTap: () => _showActions(context),
      ),
    );
  }

  void _showActions(BuildContext context) {
    showModalBottomSheet(
      context: context,
      builder: (sheetContext) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(title: Text(peer.display), subtitle: SelectableText(peer.id)),
          const Divider(height: 1),
          ListTile(
            leading: const Icon(Icons.drive_file_rename_outline),
            title: const Text('Rename (only on this device)'),
            onTap: () {
              Navigator.pop(sheetContext);
              _promptRename(context);
            },
          ),
          ListTile(
            leading: const Icon(Icons.send),
            title: const Text('Send a direct message'),
            onTap: () {
              Navigator.pop(sheetContext);
              _promptDirect(context);
            },
          ),
        ]),
      ),
    );
  }

  void _promptRename(BuildContext context) => _prompt(
        context,
        title: 'Local name for this node',
        hint: 'e.g. mum',
        onSubmit: (value) => mesh.rename(peer.id, value),
      );

  void _promptDirect(BuildContext context) => _prompt(
        context,
        title: 'Direct message to ${peer.display}',
        hint: 'routed and relayed to this node only',
        onSubmit: (value) => mesh.sendDirect(peer.id, value),
      );

  void _prompt(BuildContext context,
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
              final err = onSubmit(controller.text);
              Navigator.pop(dialogContext);
              if (err != null) {
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
