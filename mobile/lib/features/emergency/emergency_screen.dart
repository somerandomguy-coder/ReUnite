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
          const _SectionTitle('Report the area around you',
              'Safe or unsafe, over a radius you choose. Snapped to a hex cell before it '
              'is shared, so the map stays readable.'),
          _ZoneReporter(mesh: mesh),
          const SizedBox(height: 28),
          const _SectionTitle('Reported zones',
              'Both vote counts are shown. A tie reads unsafe.'),
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

/// Two questions, in this order: is it safe, and how far does that reach.
///
/// The verdict is deliberately binary. A 0-4 scale asked something nobody can answer
/// under stress - "is this a 2 or a 3?" has no defensible answer at 3 a.m. in a flooded
/// street - and averaging the answers turned four people disagreeing into one amber
/// number nobody had said.
class _ZoneReporterState extends State<_ZoneReporter> {
  final _length = TextEditingController(text: '500');
  bool? _safe;
  bool _sending = false;

  RadiusUnit get _unit => widget.mesh.lastRadiusUnit;
  set _unit(RadiusUnit u) => widget.mesh.lastRadiusUnit = u;

  @override
  void dispose() {
    _length.dispose();
    super.dispose();
  }

  int? get _radiusM {
    final value = double.tryParse(_length.text.trim());
    if (value == null || value <= 0) return null;
    final metres = _unit.toMetres(value);
    if (metres < kMinRadiusM || metres > kMaxRadiusM) return null;
    return metres;
  }

  Future<void> _send() async {
    final radius = _radiusM;
    if (_safe == null || radius == null || _sending) return;
    setState(() => _sending = true);
    final err = await widget.mesh.reportZoneHere(_safe!, radius);
    if (!mounted) return;
    setState(() => _sending = false);
    _snack(
      context,
      err ??
          'Reported ${_safe! ? "safe" : "unsafe"} within ${formatRadius(radius)} '
              'to the mesh.',
    );
  }

  @override
  Widget build(BuildContext context) {
    final radius = _radiusM;
    final ready = _safe != null && radius != null;

    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: const Color(0xFF23282E),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        const Text('Is it safe where you are?',
            style: TextStyle(fontWeight: FontWeight.bold)),
        const SizedBox(height: 10),
        Row(children: [
          Expanded(
            child: _VerdictButton(
              label: 'SAFE',
              icon: Icons.check_circle_outline,
              colour: Colors.greenAccent,
              selected: _safe == true,
              onTap: () => setState(() => _safe = true),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: _VerdictButton(
              label: 'UNSAFE',
              icon: Icons.dangerous_outlined,
              colour: Colors.redAccent,
              selected: _safe == false,
              onTap: () => setState(() => _safe = false),
            ),
          ),
        ]),
        const SizedBox(height: 16),
        const Text('Covering a radius of', style: TextStyle(fontSize: 13)),
        const SizedBox(height: 6),
        Row(children: [
          SizedBox(
            width: 110,
            child: TextField(
              controller: _length,
              keyboardType: const TextInputType.numberWithOptions(decimal: true),
              decoration: const InputDecoration(
                isDense: true,
                border: OutlineInputBorder(),
                contentPadding: EdgeInsets.symmetric(horizontal: 10, vertical: 12),
              ),
              onChanged: (_) => setState(() {}),
            ),
          ),
          const SizedBox(width: 10),
          DropdownButton<RadiusUnit>(
            value: _unit,
            onChanged: (u) => setState(() => _unit = u ?? _unit),
            items: [
              for (final u in RadiusUnit.values)
                DropdownMenuItem(value: u, child: Text(u.label)),
            ],
          ),
        ]),
        const SizedBox(height: 6),
        // Show what is about to be claimed, before it is claimed. The number the user
        // typed is not the number that travels - the position is snapped to a hex cell
        // first - and hiding that would make the map look more precise than it is.
        Text(
          radius == null
              ? 'Enter $kMinRadiusM m to ${kMaxRadiusM ~/ 1000} km.'
              : 'You are vouching for everything within ${formatRadius(radius)} of you. '
                  'Your exact position is snapped to a hex cell before it is sent.',
          style: TextStyle(
            fontSize: 11,
            color: radius == null ? Colors.orangeAccent : Colors.grey,
          ),
        ),
        const SizedBox(height: 12),
        SizedBox(
          width: double.infinity,
          child: FilledButton.icon(
            icon: const Icon(Icons.add_location_alt),
            label: Text(_sending ? 'Sending...' : 'Report this area'),
            onPressed: ready && !_sending ? _send : null,
          ),
        ),
      ]),
    );
  }
}

class _VerdictButton extends StatelessWidget {
  final String label;
  final IconData icon;
  final Color colour;
  final bool selected;
  final VoidCallback onTap;

  const _VerdictButton({
    required this.label,
    required this.icon,
    required this.colour,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(8),
      child: Container(
        padding: const EdgeInsets.symmetric(vertical: 14),
        decoration: BoxDecoration(
          color: selected ? colour.withValues(alpha: 0.22) : Colors.transparent,
          border: Border.all(
            color: selected ? colour : Colors.grey.shade700,
            width: selected ? 2 : 1,
          ),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Column(children: [
          Icon(icon, color: selected ? colour : Colors.grey, size: 26),
          const SizedBox(height: 4),
          Text(label,
              style: TextStyle(
                color: selected ? colour : Colors.grey,
                fontWeight: FontWeight.bold,
              )),
        ]),
      ),
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
    final colour = zone.isSafe ? Colors.greenAccent : Colors.redAccent;
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
          Text(zone.isSafe ? 'SAFE' : 'UNSAFE',
              style: TextStyle(color: colour, fontWeight: FontWeight.bold)),
          const SizedBox(width: 8),
          Text('within ${formatRadius(zone.radiusM)}',
              style: const TextStyle(fontSize: 12, color: Colors.grey)),
        ]),
        subtitle: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          // Both counts, always. "5 say safe" and "5 say safe, 4 say unsafe" are
          // different claims and must not render the same way.
          Row(children: [
            _VoteChip(count: zone.safeVotes, colour: Colors.greenAccent, label: 'safe'),
            const SizedBox(width: 6),
            _VoteChip(count: zone.unsafeVotes, colour: Colors.redAccent, label: 'unsafe'),
            if (zone.contested) ...[
              const SizedBox(width: 8),
              const Text('contested',
                  style: TextStyle(
                      fontSize: 11,
                      color: Colors.orangeAccent,
                      fontWeight: FontWeight.bold)),
            ] else if (!zone.verified) ...[
              const SizedBox(width: 8),
              const Text('unverified',
                  style: TextStyle(fontSize: 11, color: Colors.orangeAccent)),
            ],
          ]),
          const SizedBox(height: 2),
          Text(
            '${zone.lat.toStringAsFixed(5)}, ${zone.lon.toStringAsFixed(5)}'
            ' · ${formatAge(zone.ageMs)}${zone.mine ? ' · you reported this' : ''}',
            style: const TextStyle(fontSize: 11),
          ),
        ]),
        isThreeLine: true,
        trailing: Text(zone.cell.substring(0, 6),
            style: const TextStyle(fontSize: 10, color: Colors.grey)),
      ),
    );
  }
}

class _VoteChip extends StatelessWidget {
  final int count;
  final Color colour;
  final String label;
  const _VoteChip({required this.count, required this.colour, required this.label});

  @override
  Widget build(BuildContext context) {
    final dim = count == 0;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        color: colour.withValues(alpha: dim ? 0.06 : 0.18),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Text('$count $label',
          style: TextStyle(
            fontSize: 11,
            color: dim ? Colors.grey : colour,
            fontWeight: dim ? FontWeight.normal : FontWeight.bold,
          )),
    );
  }
}

void _snack(BuildContext context, String message) {
  if (!context.mounted) return;
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(message)));
}
