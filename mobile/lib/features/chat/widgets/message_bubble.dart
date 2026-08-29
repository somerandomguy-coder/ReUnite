import 'package:flutter/material.dart';
import '../../../models/mesh_models.dart';

class MessageBubble extends StatelessWidget {
  final ChatMessage message;
  const MessageBubble({super.key, required this.message});

  @override
  Widget build(BuildContext context) {
    // Notices and warnings are mesh chatter, not conversation: they get a quiet
    // centred line so they never compete with what a person actually said.
    if (message.kind == ChatKind.notice || message.kind == ChatKind.warning) {
      final colour = message.kind == ChatKind.warning ? Colors.orangeAccent : Colors.grey;
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Center(
          child: Text('${message.from} ${message.text}',
              textAlign: TextAlign.center,
              style: TextStyle(color: colour, fontSize: 12)),
        ),
      );
    }

    if (message.kind == ChatKind.sos) {
      return Container(
        margin: const EdgeInsets.symmetric(vertical: 6),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: Colors.red.shade900,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: Colors.redAccent),
        ),
        child: Row(children: [
          const Icon(Icons.warning_amber_rounded, color: Colors.white),
          const SizedBox(width: 10),
          Expanded(
            child: Text('${message.from}: ${message.text}',
                style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold)),
          ),
        ]),
      );
    }

    if (message.kind == ChatKind.status) {
      return Padding(
        padding: const EdgeInsets.symmetric(vertical: 4),
        child: Row(children: [
          const Icon(Icons.campaign, size: 16, color: Colors.amber),
          const SizedBox(width: 6),
          Expanded(
            child: Text('${message.from}: ${message.text}',
                style: const TextStyle(color: Colors.amber, fontWeight: FontWeight.w600)),
          ),
        ]),
      );
    }

    final mine = message.isMine;
    return Align(
      alignment: mine ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        constraints: const BoxConstraints(maxWidth: 320),
        decoration: BoxDecoration(
          color: mine ? Colors.teal.shade700 : const Color(0xFF2A2A2A),
          borderRadius: BorderRadius.circular(10),
          border: message.kind == ChatKind.direct
              ? Border.all(color: Colors.purpleAccent, width: 1)
              : null,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(mainAxisSize: MainAxisSize.min, children: [
              Text(message.from,
                  style: const TextStyle(fontWeight: FontWeight.bold, fontSize: 12)),
              if (message.kind == ChatKind.direct) ...[
                const SizedBox(width: 6),
                const Text('direct',
                    style: TextStyle(fontSize: 10, color: Colors.purpleAccent)),
              ],
              if (message.hops != null) ...[
                const SizedBox(width: 6),
                Text('${message.hops}h',
                    style: const TextStyle(fontSize: 10, color: Colors.grey)),
              ],
            ]),
            const SizedBox(height: 2),
            Text(message.text),
          ],
        ),
      ),
    );
  }
}
