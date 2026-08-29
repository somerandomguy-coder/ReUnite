import 'package:flutter/material.dart';
import '../../../services/mesh_service.dart';

class MessageBubble extends StatelessWidget {
  final ChatMessageModel message;

  const MessageBubble({super.key, required this.message});

  @override
  Widget build(BuildContext context) {
    final isLocation = message.type == MessageType.location;
    final isSos = message.type == MessageType.sos;

    return Align(
      alignment: message.isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        decoration: BoxDecoration(
          color: isSos
              ? (message.isMe ? Colors.red.shade900 : Colors.redAccent.shade700)
              : isLocation
                  ? (message.isMe ? Colors.teal.shade800 : Colors.teal.shade900)
                  : (message.isMe ? Colors.amber.shade800 : const Color(0xFF2A2A2A)),
          borderRadius: BorderRadius.circular(16),
          border: isSos
              ? Border.all(color: Colors.redAccent, width: 2)
              : isLocation
                  ? Border.all(color: Colors.cyan, width: 1)
                  : null,
          boxShadow: isSos
              ? [
                  BoxShadow(
                    color: Colors.red.withOpacity(0.5),
                    blurRadius: 8,
                    spreadRadius: 2,
                  )
                ]
              : null,
        ),
        child: Column(
          crossAxisAlignment:
              message.isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (!message.isMe) ...[
                  Text(
                    message.senderName,
                    style: TextStyle(
                      color: isSos ? Colors.yellowAccent : Colors.cyan,
                      fontSize: 12,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  const SizedBox(width: 8),
                ],
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                  decoration: BoxDecoration(
                    color: Colors.black26,
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Text(
                    isSos ? '🚨 SOS BEACON (0.3s)' : (message.hops == 1 ? '1 hop (direct)' : '${message.hops} hops'),
                    style: const TextStyle(
                      color: Colors.white70,
                      fontSize: 10,
                    ),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 4),
            if (isSos) ...[
              const Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.warning_amber_rounded, color: Colors.yellowAccent, size: 20),
                  SizedBox(width: 6),
                  Text(
                    'HIGH PRIORITY SOS BEACON',
                    style: TextStyle(
                      color: Colors.yellowAccent,
                      fontWeight: FontWeight.bold,
                      fontSize: 13,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              Text(
                message.text,
                style: const TextStyle(color: Colors.white, fontSize: 15, fontWeight: FontWeight.bold),
              ),
              if (message.lat != null && message.lon != null) ...[
                const SizedBox(height: 4),
                Text(
                  'GPS: ${message.lat!.toStringAsFixed(5)}, ${message.lon!.toStringAsFixed(5)}',
                  style: const TextStyle(
                    color: Colors.yellowAccent,
                    fontFamily: 'monospace',
                    fontSize: 12,
                  ),
                ),
              ],
            ] else if (isLocation) ...[
              const Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.location_on, color: Colors.amber, size: 18),
                  SizedBox(width: 4),
                  Text(
                    'GPS Position Shared',
                    style: TextStyle(
                      color: Colors.amber,
                      fontWeight: FontWeight.bold,
                      fontSize: 13,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              Text(
                'Lat: ${message.lat?.toStringAsFixed(5) ?? "N/A"}\nLon: ${message.lon?.toStringAsFixed(5) ?? "N/A"}',
                style: const TextStyle(
                  color: Colors.white,
                  fontFamily: 'monospace',
                  fontSize: 13,
                ),
              ),
            ] else
              Text(
                message.text,
                style: const TextStyle(color: Colors.white, fontSize: 15),
              ),
            const SizedBox(height: 4),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  message.timestamp,
                  style: const TextStyle(color: Colors.white54, fontSize: 10),
                ),
                if (message.isMe) ...[
                  const SizedBox(width: 4),
                  Icon(
                    message.status == MessageStatus.delivered
                        ? Icons.done_all
                        : Icons.done,
                    color: message.status == MessageStatus.delivered
                        ? Colors.cyanAccent
                        : Colors.white54,
                    size: 14,
                  ),
                ],
              ],
            ),
          ],
        ),
      ),
    );
  }
}
