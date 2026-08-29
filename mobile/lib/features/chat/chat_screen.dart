import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../../services/mesh_service.dart';
import 'widgets/message_bubble.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  final TextEditingController _textController = TextEditingController();

  @override
  Widget build(BuildContext context) {
    final meshService = Provider.of<MeshService>(context);

    return Scaffold(
      appBar: AppBar(
        title: Text('Emergency Mesh [${meshService.activeNetwork}]'),
        actions: [
          IconButton(
            icon: const Icon(Icons.info_outline),
            onPressed: () {
              showModalBottomSheet(
                context: context,
                builder: (_) => Container(
                  padding: const EdgeInsets.all(16),
                  child: Text('Node ID: ${meshService.nodeId}\nTransport: BLE / P2P Mesh'),
                ),
              );
            },
          ),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: meshService.messages.isEmpty
                ? const Center(
                    child: Text(
                      'No messages yet in [default]\nType a message or tap 📍 to share GPS over BLE',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: Colors.grey),
                    ),
                  )
                : ListView.builder(
                    padding: const EdgeInsets.all(12),
                    itemCount: meshService.messages.length,
                    itemBuilder: (context, index) {
                      final msg = meshService.messages[index];
                      return MessageBubble(message: msg);
                    },
                  ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8.0, vertical: 4.0),
            color: const Color(0xFF1E1E1E),
            child: Row(
              children: [
                IconButton(
                  tooltip: 'Share Offline GPS Location',
                  icon: const Icon(Icons.my_location, color: Colors.cyan),
                  onPressed: () async {
                    ScaffoldMessenger.of(context).showSnackBar(
                      const SnackBar(
                        content: Text('Sharing GPS location over BLE mesh...'),
                        duration: Duration(seconds: 1),
                      ),
                    );
                    await meshService.shareCurrentLocation();
                  },
                ),
                Expanded(
                  child: TextField(
                    controller: _textController,
                    decoration: const InputDecoration(
                      hintText: 'Broadcast message to mesh...',
                      border: InputBorder.none,
                    ),
                    onSubmitted: (val) {
                      meshService.sendMessage(val);
                      _textController.clear();
                    },
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.send, color: Colors.amber),
                  onPressed: () {
                    meshService.sendMessage(_textController.text);
                    _textController.clear();
                  },
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
