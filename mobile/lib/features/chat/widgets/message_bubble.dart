import 'package:flutter/material.dart';
import '../../../services/mesh_service.dart';

class MessageBubble extends StatelessWidget {
  final ChatMessageModel message;

  const MessageBubble({super.key, required this.message});

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: message.isMe ? Alignment.centerRight : Alignment.centerLeft,
      child: Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        decoration: BoxDecoration(
          color: message.isMe ? Colors.amber.shade800 : const Color(0xFF2A2A2A),
          borderRadius: BorderRadius.circular(16),
        ),
        child: Column(
          crossAxisAlignment:
              message.isMe ? CrossAxisAlignment.end : CrossAxisAlignment.start,
          children: [
            if (!message.isMe)
              Text(
                message.senderName,
                style: const TextStyle(
                  color: Colors.cyan,
                  fontSize: 12,
                  fontWeight: FontWeight.bold,
                ),
              ),
            Text(
              message.text,
              style: const TextStyle(color: Colors.white, fontSize: 15),
            ),
            const SizedBox(height: 2),
            Text(
              message.timestamp,
              style: const TextStyle(color: Colors.white54, fontSize: 10),
            ),
          ],
        ),
      ),
    );
  }
}
