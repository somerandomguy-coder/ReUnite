import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import '../../models/mesh_models.dart';
import '../../services/mesh_service.dart';

class EmergencyScreen extends StatelessWidget {
  const EmergencyScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final mesh = context.watch<MeshService>();
    return Scaffold(
      appBar: AppBar(title: const Text('Emergency')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          const _SosDisclaimer(),
          const SizedBox(height: 12),
          _SosSlider(mesh: mesh),
          const SizedBox(height: 28),
          const _SectionTitle('Send a status', 'One tap. One byte on the mesh.'),
          _PanicButtons(mesh: mesh),
          const SizedBox(height: 28),
          const _SectionTitle('Report how safe it is here',
              'Snapped to a hex cell before it is shared, so the map stays readable.'),
          _ZoneReporter(mesh: mesh),
          const SizedBox(height: 28),
          const _SectionTitle('Safe zones', 'Consensus is how many people verified a zone.'),
          _Heatmap(mesh: mesh),
          const SizedBox(height: 32),
        ],
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  final String title;
  final String subtitle;
  const _SectionTitle(this.title, this.subtitle);

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(title, style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 16)),
        Text(subtitle, style: const TextStyle(color: Colors.grey, fontSize: 12)),
      ]),
    );
  }
}

/// plan.md §3.2 requires this be unmissable: the in-network SOS is not the phone's
/// emergency SOS and never calls anyone.
class _SosDisclaimer extends StatelessWidget {
  const _SosDisclaimer();

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Colors.blueGrey.shade900,
        borderRadius: BorderRadius.circular(8),
      ),
      child: const Row(children: [
        Icon(Icons.info_outline, size: 18, color: Colors.cyan),
        SizedBox(width: 10),
        Expanded(
          child: Text(
            'SOS here alerts only the people on this mesh. It does NOT call emergency '
            'services and does not use the phone network.',
            style: TextStyle(fontSize: 12),
          ),
        ),
      ]),
    );
  }
}

/// Slide to activate, never a tap: an accidental SOS is the failure mode that matters.
class _SosSlider extends StatefulWidget {
  final MeshService mesh;
  const _SosSlider({required this.mesh});

  @override
  State<_SosSlider> createState() => _SosSliderState();
}

class _SosSliderState extends State<_SosSlider> {
  double _value = 0;

  @override
  Widget build(BuildContext context) {
    final active = widget.mesh.sosActive;
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: active ? Colors.red.shade900 : const Color(0xFF23282E),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: active ? Colors.redAccent : Colors.white12, width: 2),
      ),
      child: Column(children: [
        Icon(active ? Icons.warning_amber_rounded : Icons.sos,
            size: 40, color: active ? Colors.white : Colors.redAccent),
        const SizedBox(height: 8),
        Text(
          active ? 'SOS IS ACTIVE' : 'Slide to broadcast SOS',
          style: TextStyle(
            fontWeight: FontWeight.bold,
            fontSize: 18,
            color: active ? Colors.white : Colors.redAccent,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          active
              ? 'Every node in range has been alerted, and relays are carrying it further.'
              : 'Sent at a longer range than normal traffic.',
          textAlign: TextAlign.center,
          style: const TextStyle(fontSize: 12, color: Colors.white70),
        ),
        const SizedBox(height: 12),
        if (!active)
          Slider(
            value: _value,
            activeColor: Colors.redAccent,
            onChanged: (v) => setState(() => _value = v),
            onChangeEnd: (v) {
              if (v > 0.95) {
                final err = widget.mesh.setSos(true);
                _snack(context, err ?? 'SOS broadcast to the mesh');
              }
              setState(() => _value = 0);
            },
          )
        else
          FilledButton.icon(
            style: FilledButton.styleFrom(backgroundColor: Colors.white24),
            icon: const Icon(Icons.close),
            label: const Text('Stand down / clear SOS'),
            onPressed: () {
              final err = widget.mesh.setSos(false);
              _snack(context, err ?? 'SOS cleared');
            },
          ),
      ]),
    );
  }
}

class _PanicButtons extends StatelessWidget {
  final MeshService mesh;
  const _PanicButtons({required this.mesh});

  static const _icons = {
    1: Icons.check_circle,
    2: Icons.medical_services,
    3: Icons.local_drink,
    4: Icons.report_problem,
    5: Icons.directions_walk,
    6: Icons.home,
    7: Icons.block,
  };

  @override
  Widget build(BuildContext context) {
    return Column(children: [
      Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final s in mesh.statusCodes)
            SizedBox(
              width: 165,
              height: 62,
              child: FilledButton.icon(
                style: FilledButton.styleFrom(
                  backgroundColor:
                      mesh.myStatus == s.code ? Colors.amber.shade800 : const Color(0xFF2A2F35),
                  alignment: Alignment.centerLeft,
                ),
                icon: Icon(_icons[s.code] ?? Icons.campaign, size: 20),
                label: Text(s.text, style: const TextStyle(fontSize: 12)),
                onPressed: () {
                  final err = mesh.setStatus(s.code);
                  _snack(context, err ?? 'sent: ${s.text}');
                },
              ),
            ),
        ],
      ),
      if (mesh.myStatus != null)
        Padding(
          padding: const EdgeInsets.only(top: 8),
          child: TextButton(
            onPressed: () {
              final err = mesh.setStatus(0);
              _snack(context, err ?? 'status cleared');
            },
            child: const Text('Clear my status'),
          ),
        ),
    ]);
  }
}

class _ZoneReporter extends StatefulWidget {
  final MeshService mesh;
  const _ZoneReporter({required this.mesh});

  @override
  State<_ZoneReporter> createState() => _ZoneReporterState();
}

class _ZoneReporterState extends State<_ZoneReporter> {
  double _level = 4;

  static const _labels = ['Dangerous', 'Risky', 'Unclear', 'Mostly safe', 'Safe'];

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: const Color(0xFF23282E),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(children: [
        Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
          const Text('How safe is where you are?'),
          Text(_labels[_level.round()],
              style: TextStyle(
                  color: Color.lerp(Colors.redAccent, Colors.greenAccent, _level / 4),
                  fontWeight: FontWeight.bold)),
        ]),
        Slider(
          value: _level,
          min: 0,
          max: 4,
          divisions: 4,
          activeColor: Color.lerp(Colors.redAccent, Colors.greenAccent, _level / 4),
          label: _labels[_level.round()],
          onChanged: (v) => setState(() => _level = v),
        ),
        SizedBox(
          width: double.infinity,
          child: FilledButton.icon(
            icon: const Icon(Icons.add_location_alt),
            label: const Text('Report this area'),
            onPressed: () async {
              final err = await widget.mesh.reportZoneHere(_level.round());
              if (context.mounted) _snack(context, err ?? 'zone reported to the mesh');
            },
          ),
        ),
      ]),
    );
  }
}

class _Heatmap extends StatelessWidget {
  final MeshService mesh;
  const _Heatmap({required this.mesh});

  @override
  Widget build(BuildContext context) {
    if (mesh.zones.isEmpty) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 20),
        child: Text('No safety reports yet.',
            textAlign: TextAlign.center, style: TextStyle(color: Colors.grey)),
      );
    }
    return Column(children: mesh.zones.map((z) => _ZoneTile(zone: z)).toList());
  }
}

class _ZoneTile extends StatelessWidget {
  final Zone zone;
  const _ZoneTile({required this.zone});

  @override
  Widget build(BuildContext context) {
    final colour = Color.lerp(Colors.redAccent, Colors.greenAccent, zone.levelScaled / 4)!;
    // An unverified zone must not look like a confirmed one, so a single report is drawn
    // washed out and labelled. plan.md §3.2.
    final confidence = zone.verified ? 1.0 : 0.45;
    return Card(
      color: const Color(0xFF23282E),
      child: ListTile(
        leading: Container(
          width: 12,
          height: 44,
          decoration: BoxDecoration(
            color: colour.withValues(alpha: confidence),
            borderRadius: BorderRadius.circular(4),
          ),
        ),
        title: Row(children: [
          Text('${zone.levelScaled.toStringAsFixed(1)}/4',
              style: TextStyle(color: colour, fontWeight: FontWeight.bold)),
          const SizedBox(width: 10),
          if (zone.verified)
            Text('${zone.consensus} verifying',
                style: const TextStyle(fontSize: 12, fontWeight: FontWeight.bold))
          else
            const Text('1 report — unverified',
                style: TextStyle(fontSize: 12, color: Colors.orangeAccent)),
        ]),
        subtitle: Text(
          '${zone.lat.toStringAsFixed(5)}, ${zone.lon.toStringAsFixed(5)}'
          ' · ${formatAge(zone.ageMs)}${zone.mine ? ' · you reported this' : ''}',
          style: const TextStyle(fontSize: 11),
        ),
        trailing: Text(zone.cell.substring(0, 6),
            style: const TextStyle(fontSize: 10, color: Colors.grey)),
      ),
    );
  }
}

void _snack(BuildContext context, String message) {
  if (!context.mounted) return;
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(message)));
}
