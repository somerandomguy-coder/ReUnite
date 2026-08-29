import 'package:flutter/material.dart';
import 'package:flutter_map/flutter_map.dart';
import 'package:latlong2/latlong.dart';
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

    return DefaultTabController(
      length: 2,
      child: Scaffold(
        appBar: AppBar(
          title: const Text('Interactive Disaster Map & Radar'),
          bottom: const TabBar(
            indicatorColor: Colors.greenAccent,
            tabs: [
              Tab(icon: Icon(Icons.map), text: "Interactive Map"),
              Tab(icon: Icon(Icons.radar), text: "Peer Radar"),
            ],
          ),
          actions: [
            IconButton(
              tooltip: 'Refresh Map & Mesh',
              icon: const Icon(Icons.refresh),
              onPressed: mesh.refresh,
            ),
          ],
        ),
        body: TabBarView(
          physics: const NeverScrollableScrollPhysics(), // Disable swipe so map drag works seamlessly
          children: [
            _InteractiveMapView(mesh: mesh, me: me),
            _RadarView(mesh: mesh, me: me),
          ],
        ),
      ),
    );
  }
}

class _InteractiveMapView extends StatefulWidget {
  final MeshService mesh;
  final Whoami? me;

  const _InteractiveMapView({required this.mesh, required this.me});

  @override
  State<_InteractiveMapView> createState() => _InteractiveMapViewState();
}

class _InteractiveMapViewState extends State<_InteractiveMapView> {
  final MapController _mapController = MapController();

  @override
  Widget build(BuildContext context) {
    final myLat = widget.me?.lat ?? -33.8688;
    final myLon = widget.me?.lon ?? 151.2093;
    final center = LatLng(myLat, myLon);

    final markers = <Marker>[];

    // Current user position marker
    markers.add(
      Marker(
        point: center,
        width: 50,
        height: 50,
        child: const Column(
          children: [
            Icon(Icons.person_pin_circle, color: Colors.greenAccent, size: 36),
            Text("YOU",
                style: TextStyle(
                    color: Colors.greenAccent,
                    fontWeight: FontWeight.bold,
                    fontSize: 10,
                    backgroundColor: Colors.black87)),
          ],
        ),
      ),
    );

    // Add safe zone markers and hazard markers from mesh.zones
    for (final z in widget.mesh.zones) {
      final isSafe = z.level >= 2;
      markers.add(
        Marker(
          point: LatLng(z.lat, z.lon),
          width: 44,
          height: 44,
          child: Tooltip(
            message: isSafe ? "🟢 Safe Zone (${z.consensus} Node Consensus)" : "⚠️ Hazard Area",
            child: Icon(
              isSafe ? Icons.verified_user : Icons.dangerous,
              color: isSafe ? Colors.greenAccent : Colors.redAccent,
              size: 32,
            ),
          ),
        ),
      );
    }

    // Add peer position markers
    for (final p in widget.mesh.peers) {
      if (p.hasPosition) {
        markers.add(
          Marker(
            point: LatLng(p.lat!, p.lon!),
            width: 40,
            height: 40,
            child: Icon(
              p.sos ? Icons.warning_amber_rounded : Icons.account_circle,
              color: p.sos ? Colors.redAccent : Colors.cyanAccent,
              size: 28,
            ),
          ),
        );
      }
    }

    return Stack(
      children: [
        FlutterMap(
          mapController: _mapController,
          options: MapOptions(
            initialCenter: center,
            initialZoom: 15.0,
            maxZoom: 18.0,
            minZoom: 3.0,
          ),
          children: [
            TileLayer(
              urlTemplate: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
              userAgentPackageName: 'com.reunite.reunite_mobile',
              errorTileCallback: (tile, error, stackTrace) {
                // Silently swallow tile load errors off-grid
              },
            ),
            MarkerLayer(markers: markers),
          ],
        ),

        // Floating Control Card at Top
        Positioned(
          top: 12,
          left: 12,
          right: 12,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: BoxDecoration(
              color: Colors.black87,
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: Colors.greenAccent.withOpacity(0.5)),
            ),
            child: Row(
              children: [
                const Icon(Icons.autorenew, color: Colors.greenAccent, size: 20),
                const SizedBox(width: 8),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Text(
                        "2-Min Auto GPS Safe Breadcrumb Active",
                        style: TextStyle(
                            color: Colors.greenAccent,
                            fontWeight: FontWeight.bold,
                            fontSize: 12),
                      ),
                      Text(
                        "${widget.mesh.zones.length} safe/hazard zones merged over BLE",
                        style: const TextStyle(color: Colors.grey, fontSize: 10),
                      ),
                    ],
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.my_location, color: Colors.cyanAccent),
                  tooltip: "Center on My Location",
                  onPressed: () {
                    _mapController.move(center, 16.0);
                  },
                ),
              ],
            ),
          ),
        ),

        // Floating Action Buttons at Bottom Right
        Positioned(
          bottom: 16,
          right: 16,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              FloatingActionButton.extended(
                heroTag: "btn_safe",
                backgroundColor: Colors.green.shade800,
                foregroundColor: Colors.white,
                icon: const Icon(Icons.check_circle_outline),
                label: const Text("Mark Safe"),
                onPressed: () async {
                  final err = await widget.mesh.shareCurrentLocation();
                  if (context.mounted) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(
                        content: Text(err ?? "🟢 Current location marked Safe on GeoJSON map!"),
                      ),
                    );
                  }
                },
              ),
              const SizedBox(height: 10),
              FloatingActionButton.extended(
                heroTag: "btn_hazard",
                backgroundColor: Colors.red.shade900,
                foregroundColor: Colors.white,
                icon: const Icon(Icons.warning_amber_rounded),
                label: const Text("Report Hazard"),
                onPressed: () {
                  widget.mesh.setStatus(7); // Hazard status code
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(
                      content: Text("⚠️ Hazard / Danger Spot reported to BLE mesh map!"),
                      backgroundColor: Colors.redAccent,
                    ),
                  );
                },
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _RadarView extends StatelessWidget {
  final MeshService mesh;
  final Whoami? me;

  const _RadarView({required this.mesh, required this.me});

  @override
  Widget build(BuildContext context) {
    return ListView(
      children: [
        PeerRadar(peers: mesh.peers, myLat: me?.lat, myLon: me?.lon),
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            'Compass mode: bearing and distance from you over off-grid BLE Radio Mesh.',
            style: TextStyle(color: Colors.grey, fontSize: 11),
          ),
        ),
        const Divider(height: 24),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              const Text('Reachable Peers (BLE Mesh)',
                  style: TextStyle(fontWeight: FontWeight.bold)),
              Text(
                '${mesh.livePeers.length} live · ${mesh.ghosts.length} ghost',
                style: const TextStyle(color: Colors.cyanAccent, fontSize: 12),
              ),
            ],
          ),
        ),
        if (mesh.peers.isEmpty)
          const Padding(
            padding: EdgeInsets.all(28),
            child: Text(
              'Scanning for nearby BLE survivors...\n\n'
              'Ensure Bluetooth is turned on to reach surrounding phones.',
              textAlign: TextAlign.center,
              style: TextStyle(color: Colors.grey),
            ),
          ),
        ...mesh.peers.map((p) => _PeerTile(peer: p, mesh: mesh)),
        const SizedBox(height: 24),
      ],
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
      if (peer.rssi != null) '${peer.rssi}dBm',
      formatAge(DateTime.now().millisecondsSinceEpoch - peer.lastSeenMs),
    ].join(' · ');

    return Opacity(
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
              const Text('🚨 SOS EMERGENCY BEACON',
                  style: TextStyle(color: Colors.redAccent, fontWeight: FontWeight.bold, fontSize: 12)),
            if (peer.status != null)
              Text(mesh.describeStatus(peer.status!),
                  style: const TextStyle(color: Colors.greenAccent, fontSize: 12)),
          ],
        ),
        trailing: Text(
          peer.distanceM != null ? formatDistance(peer.distanceM!) : '—',
          style: TextStyle(color: peer.ghost ? Colors.grey : Colors.greenAccent),
        ),
      ),
    );
  }
}
