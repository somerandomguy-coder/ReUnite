import 'dart:math' as math;
import 'package:flutter/material.dart';

import '../../../models/mesh_models.dart';

/// Compass / Grid mode.
///
/// plan.md §4 step 2.2 makes this a hard requirement, not a fallback: a phone in a
/// disaster will usually have no offline map tiles, and the screen must still be useful.
/// So this shows bearing and distance to every peer that has a position, relative to us
/// at the centre - which is what someone walking towards a person actually needs.
class PeerRadar extends StatelessWidget {
  final List<Peer> peers;
  final double? myLat;
  final double? myLon;

  const PeerRadar({super.key, required this.peers, this.myLat, this.myLon});

  @override
  Widget build(BuildContext context) {
    final located = peers.where((p) => p.hasPosition).toList();
    if (myLat == null || myLon == null) {
      return const _RadarMessage(
        icon: Icons.location_disabled,
        title: 'No position for this device',
        detail: 'Share your GPS from the Chat tab, or set one manually, and peers will be '
            'placed by bearing and distance around you.',
      );
    }
    if (located.isEmpty) {
      return const _RadarMessage(
        icon: Icons.radar,
        title: 'No peer has shared a position yet',
        detail: 'Peers still appear in the list below as soon as they are heard.',
      );
    }
    return AspectRatio(
      aspectRatio: 1,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: CustomPaint(
          painter: _RadarPainter(peers: located, myLat: myLat!, myLon: myLon!),
          child: const SizedBox.expand(),
        ),
      ),
    );
  }
}

class _RadarMessage extends StatelessWidget {
  final IconData icon;
  final String title;
  final String detail;
  const _RadarMessage({required this.icon, required this.title, required this.detail});

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 190,
      color: const Color(0xFF141C24),
      padding: const EdgeInsets.symmetric(horizontal: 28),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(icon, size: 44, color: Colors.cyan),
          const SizedBox(height: 10),
          Text(title, style: const TextStyle(fontWeight: FontWeight.bold)),
          const SizedBox(height: 6),
          Text(detail,
              textAlign: TextAlign.center,
              style: const TextStyle(color: Colors.grey, fontSize: 12)),
        ],
      ),
    );
  }
}

class _RadarPainter extends CustomPainter {
  final List<Peer> peers;
  final double myLat;
  final double myLon;

  _RadarPainter({required this.peers, required this.myLat, required this.myLon});

  /// Initial great-circle bearing from us to a point, in degrees clockwise from north.
  double _bearing(double lat2, double lon2) {
    final p1 = myLat * math.pi / 180, p2 = lat2 * math.pi / 180;
    final dl = (lon2 - myLon) * math.pi / 180;
    final y = math.sin(dl) * math.cos(p2);
    final x = math.cos(p1) * math.sin(p2) - math.sin(p1) * math.cos(p2) * math.cos(dl);
    return (math.atan2(y, x) * 180 / math.pi + 360) % 360;
  }

  @override
  void paint(Canvas canvas, Size size) {
    final centre = Offset(size.width / 2, size.height / 2);
    final maxR = math.min(size.width, size.height) / 2 - 18;

    final grid = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = Colors.cyan.withValues(alpha: 0.25);

    // The furthest peer sets the outer ring, so the view always frames everyone.
    final furthest =
        peers.map((p) => p.distanceM ?? 0).fold<double>(50, (a, b) => math.max(a, b));

    for (var i = 1; i <= 3; i++) {
      canvas.drawCircle(centre, maxR * i / 3, grid);
      _label(canvas, Offset(centre.dx + 4, centre.dy - maxR * i / 3 - 2),
          _distanceLabel(furthest * i / 3), Colors.cyan.withValues(alpha: 0.5), 9);
    }
    canvas.drawLine(
        Offset(centre.dx, centre.dy - maxR), Offset(centre.dx, centre.dy + maxR), grid);
    canvas.drawLine(
        Offset(centre.dx - maxR, centre.dy), Offset(centre.dx + maxR, centre.dy), grid);
    _label(canvas, Offset(centre.dx - 4, centre.dy - maxR - 16), 'N', Colors.cyan, 11);

    canvas.drawCircle(centre, 6, Paint()..color = Colors.amber);

    for (final peer in peers) {
      final distance = peer.distanceM ?? 0;
      // Square-root scaling: linear buries everyone near the centre as soon as one peer
      // is far away, and log misleads about what is close.
      final r = furthest <= 0 ? 0.0 : maxR * math.sqrt(distance / furthest);
      final theta = (_bearing(peer.lat!, peer.lon!) - 90) * math.pi / 180;
      final at = centre + Offset(r * math.cos(theta), r * math.sin(theta));

      final colour = peer.sos
          ? Colors.redAccent
          : peer.ghost
              ? Colors.grey
              : Colors.cyanAccent;
      if (peer.sos) {
        canvas.drawCircle(at, 14, Paint()..color = Colors.red.withValues(alpha: 0.30));
      }
      canvas.drawCircle(at, 7, Paint()..color = colour);
      // Ghosts are hollow: a last known position is not a live one.
      if (peer.ghost) {
        canvas.drawCircle(
            at,
            7,
            Paint()
              ..style = PaintingStyle.stroke
              ..strokeWidth = 2
              ..color = Colors.white24);
      }
      _label(canvas, at + const Offset(10, -6), peer.display, colour, 10);
      _label(canvas, at + const Offset(10, 5), _distanceLabel(distance),
          Colors.white.withValues(alpha: 0.6), 9);
    }
  }

  String _distanceLabel(double m) =>
      m < 1000 ? '${m.toStringAsFixed(0)}m' : '${(m / 1000).toStringAsFixed(1)}km';

  void _label(Canvas canvas, Offset at, String text, Color colour, double size) {
    final tp = TextPainter(
      text: TextSpan(text: text, style: TextStyle(color: colour, fontSize: size)),
      textDirection: TextDirection.ltr,
    )..layout();
    tp.paint(canvas, at);
  }

  @override
  bool shouldRepaint(covariant _RadarPainter old) =>
      old.peers != peers || old.myLat != myLat || old.myLon != myLon;
}
