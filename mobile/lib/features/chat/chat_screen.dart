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
  final _controller = TextEditingController();
  final _scroll = ScrollController();

  void _send(MeshService mesh) {
    final text = _controller.text;
    if (text.trim().isEmpty) return;
    final err = mesh.sendMessage(text);
    _controller.clear();
    if (err != null && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(err)));
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) {
        _scroll.animateTo(_scroll.position.maxScrollExtent,
            duration: const Duration(milliseconds: 200), curve: Curves.easeOut);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final mesh = context.watch<MeshService>();
    return Scaffold(
      appBar: AppBar(
        title: Text('[${mesh.activeNetwork}]'),
        actions: [
          Center(
            child: Padding(
              padding: const EdgeInsets.only(right: 12),
              child: Text('${mesh.livePeers.length} peers',
                  style: const TextStyle(fontSize: 12, color: Colors.grey)),
            ),
          ),
          IconButton(
            icon: const Icon(Icons.info_outline),
            onPressed: () => showModalBottomSheet(
              context: context,
              builder: (_) => Padding(
                padding: const EdgeInsets.all(16),
                child: Column(mainAxisSize: MainAxisSize.min, children: [
                  Text('Node ID: ${mesh.nodeId}'),
                  const SizedBox(height: 6),
                  Text('Transport: ${mesh.me?.transport ?? '-'}',
                      textAlign: TextAlign.center),
                  const SizedBox(height: 6),
                  Text('Home: ${mesh.me?.home ?? '-'}',
                      style: const TextStyle(fontSize: 11, color: Colors.grey)),
                ]),
              ),
            ),
          ),
        ],
      ),
      body: Column(
        children: [
          Expanded(
            child: mesh.messages.isEmpty
                ? const Center(
                    child: Padding(
                      padding: EdgeInsets.all(24),
                      child: Text(
                        'Nothing on the mesh yet.\n\n'
                        'Anything you type is broadcast to everyone in this network.',
                        textAlign: TextAlign.center,
                        style: TextStyle(color: Colors.grey),
                      ),
                    ),
                  )
                : ListView.builder(
                    controller: _scroll,
                    padding: const EdgeInsets.all(12),
                    itemCount: mesh.messages.length,
                    itemBuilder: (_, i) => MessageBubble(message: mesh.messages[i]),
                  ),
          ),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            color: const Color(0xFF1E1E1E),
            child: SafeArea(
              top: false,
              child: Row(
                children: [
                  IconButton(
                    tooltip: 'Share my GPS position',
                    icon: const Icon(Icons.my_location, color: Colors.cyan),
                    onPressed: () async {
                      final err = await mesh.shareCurrentLocation();
                      if (context.mounted) {
                        ScaffoldMessenger.of(context).showSnackBar(SnackBar(
                          content: Text(err ?? 'position shared with the mesh'),
                        ));
                      }
                    },
                  ),
                  Expanded(
                    child: TextField(
                      controller: _controller,
                      textInputAction: TextInputAction.send,
                      decoration: const InputDecoration(
                        hintText: 'Message everyone in range...',
                        border: InputBorder.none,
                      ),
                      onSubmitted: (_) => _send(mesh),
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.send, color: Colors.amber),
                    onPressed: () => _send(mesh),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}
