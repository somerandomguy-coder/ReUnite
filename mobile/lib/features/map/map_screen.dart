import 'dart:math' as math;

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
          // Compass/Grid first, deliberately. plan.md §4 step 2.2 makes graceful
          // degradation a hard requirement: a phone in a disaster usually has no tiles
          // and no internet, so the view that works without either is the one it lands
          // on. The interactive map is the opt-in, not the default.
          bottom: const TabBar(
            indicatorColor: Colors.greenAccent,
            tabs: [
              Tab(icon: Icon(Icons.radar), text: "Peer Radar"),
              Tab(icon: Icon(Icons.map), text: "Interactive Map"),
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
            _RadarView(mesh: mesh, me: me),
            _InteractiveMapView(mesh: mesh, me: me),
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

    final myBattery = widget.me?.battery;

    // Current user position marker
    markers.add(
      Marker(
        point: center,
        width: 70,
        height: 60,
        child: Tooltip(
          message: "YOU ${myBattery != null ? '· 🔋$myBattery%' : ''}",
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.person_pin_circle, color: Colors.greenAccent, size: 34),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                decoration: BoxDecoration(
                  color: Colors.black87,
                  borderRadius: BorderRadius.circular(4),
                  border: Border.all(color: Colors.greenAccent, width: 0.8),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Text("YOU",
                        style: TextStyle(
                            color: Colors.greenAccent,
                            fontWeight: FontWeight.bold,
                            fontSize: 9)),
                    if (myBattery != null) ...[
                      const SizedBox(width: 2),
                      Icon(
                        myBattery <= 15 ? Icons.battery_alert : Icons.battery_full,
                        size: 9,
                        color: myBattery <= 15 ? Colors.redAccent : Colors.greenAccent,
                      ),
                      Text("$myBattery%",
                          style: TextStyle(
                              fontSize: 9,
                              color: myBattery <= 15 ? Colors.redAccent : Colors.white)),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );

    // Zones are drawn as circles, not pins. A pin claims a point; a report is about an
    // area, and drawing it as a point would overstate what the reporter actually said.
    final circles = _zoneCircles(widget.mesh.zones);

    // Builtin offline distance range circles centered on user position
    final offlineRangeCircles = <CircleMarker>[
      CircleMarker(
        point: center,
        radius: 500,
        useRadiusInMeter: true,
        color: Colors.cyan.withValues(alpha: 0.05),
        borderColor: Colors.cyanAccent.withValues(alpha: 0.25),
        borderStrokeWidth: 1.0,
      ),
      CircleMarker(
        point: center,
        radius: 1500,
        useRadiusInMeter: true,
        color: Colors.cyan.withValues(alpha: 0.03),
        borderColor: Colors.cyanAccent.withValues(alpha: 0.15),
        borderStrokeWidth: 1.0,
      ),
    ];

    // Add peer position markers with battery level display and tap detail sheet
    for (final p in widget.mesh.peers) {
      if (p.hasPosition) {
        final isLowBattery = p.battery != null && p.battery! <= 15;
        final battColor = isLowBattery ? Colors.redAccent : Colors.greenAccent;
        markers.add(
          Marker(
            point: LatLng(p.lat!, p.lon!),
            width: 80,
            height: 60,
            child: GestureDetector(
              onTap: () => _showPeerDetails(context, p),
              child: Tooltip(
                message: "${p.display}${p.battery != null ? ' · 🔋${p.battery}%' : ''}${p.sos ? ' · 🚨 SOS EMERGENCY' : ''}",
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      p.sos ? Icons.warning_amber_rounded : Icons.account_circle,
                      color: p.sos
                          ? Colors.redAccent
                          : (p.ghost ? Colors.grey : Colors.cyanAccent),
                      size: 26,
                    ),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
                      decoration: BoxDecoration(
                        color: Colors.black87,
                        borderRadius: BorderRadius.circular(4),
                        border: Border.all(
                          color: p.sos
                              ? Colors.redAccent
                              : (isLowBattery ? Colors.orangeAccent : Colors.cyanAccent),
                          width: 0.8,
                        ),
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Flexible(
                            child: Text(
                              p.display,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                  color: Colors.white,
                                  fontWeight: FontWeight.bold,
                                  fontSize: 9),
                            ),
                          ),
                          if (p.battery != null) ...[
                            const SizedBox(width: 2),
                            Icon(
                              isLowBattery ? Icons.battery_alert : Icons.battery_full,
                              size: 9,
                              color: battColor,
                            ),
                            Text(
                              "${p.battery}%",
                              style: TextStyle(
                                fontSize: 9,
                                fontWeight: FontWeight.w600,
                                color: battColor,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ],
                ),
              ),
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
            // Dark off-grid fallback background canvas
            Container(color: const Color(0xFF111827)),
            TileLayer(
              urlTemplate: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
              userAgentPackageName: 'com.reunite.reunite_mobile',
              errorTileCallback: (tile, error, stackTrace) {
                // Silently swallow tile load errors off-grid
              },
            ),
            // Builtin offline range rings (500m & 1500m)
            CircleLayer(circles: offlineRangeCircles),
            // Reported safety zones
            CircleLayer(circles: circles),
            MarkerLayer(markers: markers),
          ],
        ),

        Positioned(
          left: 12,
          bottom: 12,
          child: _ZoneLegend(zones: widget.mesh.zones),
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
              border: Border.all(color: Colors.greenAccent.withValues(alpha: 0.5)),
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
                        "Sharing your position every 2 minutes",
                        style: TextStyle(
                            color: Colors.greenAccent,
                            fontWeight: FontWeight.bold,
                            fontSize: 12),
                      ),
                      Text(
                        "${widget.mesh.zones.length} reported zones · "
                        "off-grid vector grid fallback active",
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

        // Floating Zoom Controls at Right Center
        Positioned(
          right: 12,
          top: 80,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              FloatingActionButton.small(
                heroTag: "btn_zoom_in",
                backgroundColor: Colors.black87,
                foregroundColor: Colors.greenAccent,
                child: const Icon(Icons.add),
                onPressed: () {
                  final zoom = _mapController.camera.zoom;
                  _mapController.move(_mapController.camera.center, zoom + 1.0);
                },
              ),
              const SizedBox(height: 6),
              FloatingActionButton.small(
                heroTag: "btn_zoom_out",
                backgroundColor: Colors.black87,
                foregroundColor: Colors.greenAccent,
                child: const Icon(Icons.remove),
                onPressed: () {
                  final zoom = _mapController.camera.zoom;
                  _mapController.move(_mapController.camera.center, zoom - 1.0);
                },
              ),
            ],
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

  void _showPeerDetails(BuildContext context, Peer peer) {
    final batt = peer.battery;
    final isLowBatt = batt != null && batt <= 15;

    showModalBottomSheet(
      context: context,
      backgroundColor: const Color(0xFF1E293B),
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (ctx) {
        return Padding(
          padding: const EdgeInsets.all(20.0),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(
                    peer.sos ? Icons.warning_amber_rounded : Icons.person_pin,
                    color: peer.sos ? Colors.redAccent : Colors.cyanAccent,
                    size: 32,
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          peer.display,
                          style: const TextStyle(
                            fontSize: 18,
                            fontWeight: FontWeight.bold,
                            color: Colors.white,
                          ),
                        ),
                        Text(
                          "ID: ${peer.id.length >= 16 ? peer.id.substring(0, 16) : peer.id}…",
                          style: const TextStyle(fontSize: 12, color: Colors.grey),
                        ),
                      ],
                    ),
                  ),
                  if (batt != null)
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                      decoration: BoxDecoration(
                        color: isLowBatt ? Colors.red.shade900 : Colors.green.shade900,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(
                            isLowBatt ? Icons.battery_alert : Icons.battery_full,
                            size: 14,
                            color: Colors.white,
                          ),
                          const SizedBox(width: 4),
                          Text(
                            "$batt%",
                            style: const TextStyle(
                              color: Colors.white,
                              fontWeight: FontWeight.bold,
                              fontSize: 12,
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
              const Divider(height: 24, color: Colors.white24),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceAround,
                children: [
                  _infoChip(
                    "Link",
                    peer.ghost ? "Ghost" : (peer.direct ? "Direct" : "Relayed"),
                    peer.direct ? Colors.greenAccent : Colors.orangeAccent,
                  ),
                  if (peer.hops != null)
                    _infoChip("Hops", "${peer.hops}", Colors.cyanAccent),
                  if (peer.rssi != null)
                    _infoChip("RSSI", "${peer.rssi} dBm", Colors.amberAccent),
                  if (peer.distanceM != null)
                    _infoChip(
                      "Distance",
                      formatDistance(peer.distanceM!),
                      Colors.greenAccent,
                    ),
                ],
              ),
              const SizedBox(height: 16),
              if (peer.sos)
                Container(
                  width: double.infinity,
                  padding: const EdgeInsets.all(10),
                  decoration: BoxDecoration(
                    color: Colors.red.shade900.withValues(alpha: 0.5),
                    borderRadius: BorderRadius.circular(8),
                    border: Border.all(color: Colors.redAccent),
                  ),
                  child: const Text(
                    "🚨 SOS EMERGENCY BEACON ACTIVE",
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      color: Colors.redAccent,
                      fontWeight: FontWeight.bold,
                      fontSize: 13,
                    ),
                  ),
                ),
            ],
          ),
        );
      },
    );
  }

  Widget _infoChip(String label, String value, Color color) {
    return Column(
      children: [
        Text(label, style: const TextStyle(color: Colors.grey, fontSize: 10)),
        const SizedBox(height: 2),
        Text(
          value,
          style: TextStyle(color: color, fontWeight: FontWeight.bold, fontSize: 13),
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
    // Naming the radio in use, rather than hard-coding "BLE", is not cosmetic: the
    // previous text told a macOS user meshing over Wi-Fi to switch on Bluetooth, which
    // would have done nothing and cost them the time it takes to try.
    final radioName = mesh.radioNames;
    return ListView(
      children: [
        // The caption goes above the radar: it says what you are looking at, and a
        // legend under a full-height square is a legend nobody scrolls to.
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 10, 16, 6),
          child: Text(
            'Compass mode: bearing and distance from you, over $radioName. '
            'Circles are reported zones, labelled safe/unsafe votes.',
            style: const TextStyle(color: Colors.grey, fontSize: 11),
          ),
        ),
        PeerRadar(
          peers: mesh.peers,
          zones: mesh.zones,
          myLat: me?.lat,
          myLon: me?.lon,
        ),
        const Divider(height: 24),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              const Text('Reachable peers',
                  style: TextStyle(fontWeight: FontWeight.bold)),
              Text(
                '${mesh.livePeers.length} live · ${mesh.ghosts.length} ghost',
                style: const TextStyle(color: Colors.cyanAccent, fontSize: 12),
              ),
            ],
          ),
        ),
        if (mesh.peers.isEmpty)
          Padding(
            padding: const EdgeInsets.all(28),
            child: Text(
              'No peers heard yet.\n\n'
              'Still listening on $radioName. Anyone who comes into range appears here '
              'on their own, with no action from you.',
              textAlign: TextAlign.center,
              style: const TextStyle(color: Colors.grey),
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

// --------------------------------------------------------------------- zone overlay

/// Opacity contributed by one report. Chosen so overlaps stack readably: one circle sits
/// at 0.18, two compose to ~0.33, five to ~0.63.
const double _kZoneAlphaPerLayer = 0.18;

/// Composited alpha never goes past this. A fully opaque overlay hides the map under it,
/// and the map under it is how somebody navigates out of the area.
const double _kZoneAlphaCap = 0.75;

const Color kSafeGreen = Color(0xFF22C55E);
const Color kUnsafeRed = Color(0xFFEF4444);

/// Alpha for a zone with [votes] people behind it.
///
/// This is the same arithmetic that repeated alpha compositing performs, applied once
/// from the vote count rather than by drawing N circles on top of each other. Drawing
/// them individually would be honest too, but it costs N draws per cell and produces
/// banding where circles share an edge.
double zoneAlpha(int votes) {
  final layers = votes.clamp(1, 20);
  final composited = 1 - math.pow(1 - _kZoneAlphaPerLayer, layers).toDouble();
  return composited.clamp(0.0, _kZoneAlphaCap);
}

/// Zones as map circles.
///
/// Unsafe draws **last**, so it lands on top wherever the two overlap, and carries a
/// solid border. Same reasoning as the core resolving a tie to unsafe: contested ground
/// must not look settled.
List<CircleMarker> _zoneCircles(List<Zone> zones) {
  final safe = zones.where((z) => z.isSafe);
  final unsafe = zones.where((z) => !z.isSafe);
  CircleMarker build(Zone z) {
    final colour = z.isSafe ? kSafeGreen : kUnsafeRed;
    return CircleMarker(
      point: LatLng(z.lat, z.lon),
      // In metres, so the circle stays geographically true through zoom instead of
      // being a fixed blob of pixels that means a different distance at every level.
      radius: z.radiusM.toDouble(),
      useRadiusInMeter: true,
      color: colour.withValues(alpha: zoneAlpha(z.totalVotes)),
      borderColor: colour.withValues(alpha: z.isSafe ? 0.5 : 0.9),
      borderStrokeWidth: z.isSafe ? 1 : 2,
    );
  }

  return [...safe.map(build), ...unsafe.map(build)];
}

/// An opacity gradient with no key is decoration, not data.
class _ZoneLegend extends StatelessWidget {
  final List<Zone> zones;
  const _ZoneLegend({required this.zones});

  @override
  Widget build(BuildContext context) {
    if (zones.isEmpty) return const SizedBox.shrink();
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: Colors.black87,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text('People reporting',
              style: TextStyle(fontSize: 10, color: Colors.grey)),
          const SizedBox(height: 4),
          Row(mainAxisSize: MainAxisSize.min, children: [
            for (final votes in [1, 3, 5])
              Padding(
                padding: const EdgeInsets.only(right: 6),
                child: Column(children: [
                  Container(
                    width: 20,
                    height: 20,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: kSafeGreen.withValues(alpha: zoneAlpha(votes)),
                      border: Border.all(
                          color: kSafeGreen.withValues(alpha: 0.5), width: 1),
                    ),
                  ),
                  Text(votes == 5 ? '5+' : '$votes',
                      style: const TextStyle(fontSize: 9, color: Colors.grey)),
                ]),
              ),
            const SizedBox(width: 6),
            Column(children: [
              Container(
                width: 20,
                height: 20,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: kUnsafeRed.withValues(alpha: zoneAlpha(3)),
                  border:
                      Border.all(color: kUnsafeRed.withValues(alpha: 0.9), width: 2),
                ),
              ),
              const Text('unsafe',
                  style: TextStyle(fontSize: 9, color: Colors.grey)),
            ]),
          ]),
        ],
      ),
    );
  }
}
