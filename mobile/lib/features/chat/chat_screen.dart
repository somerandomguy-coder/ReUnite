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
  final ScrollController _scrollController = ScrollController();

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 300),
          curve: Curves.easeOut,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final meshService = Provider.of<MeshService>(context);

    return Scaffold(
      appBar: AppBar(
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('Mesh Chat [${meshService.activeNetwork}]', style: const TextStyle(fontSize: 16)),
            Text(
              'Node: ${meshService.nodeId} • ${meshService.peers.length} Peers Active',
              style: const TextStyle(fontSize: 11, color: Colors.cyanAccent),
            ),
          ],
        ),
        actions: [
          // Red SOS Emergency Button
          ElevatedButton.icon(
            style: ElevatedButton.styleFrom(
              backgroundColor: meshService.isSosActive ? Colors.redAccent : Colors.red.shade900,
              foregroundColor: Colors.white,
              padding: const EdgeInsets.symmetric(horizontal: 10),
              elevation: meshService.isSosActive ? 8 : 2,
            ),
            icon: Icon(
              meshService.isSosActive ? Icons.warning : Icons.sos,
              color: Colors.yellowAccent,
              size: 20,
            ),
            label: Text(
              meshService.isSosActive ? 'STOP SOS' : 'SOS',
              style: const TextStyle(fontWeight: FontWeight.bold),
            ),
            onPressed: () async {
              await meshService.toggleSos();
              _scrollToBottom();
            },
          ),
          IconButton(
            icon: const Icon(Icons.info_outline),
            onPressed: () {
              showModalBottomSheet(
                context: context,
                builder: (_) => Container(
                  padding: const EdgeInsets.all(16),
                  child: Text(
                    'Node ID: ${meshService.nodeId}\nNetwork: ${meshService.activeNetwork}\nTransport: BLE Multi-Hop P2P Mesh\nSOS Mode: ${meshService.isSosActive ? "ACTIVE (0.3s)" : "INACTIVE"}',
                    style: const TextStyle(fontSize: 14),
                  ),
                ),
              );
            },
          ),
        ],
      ),
      body: Column(
        children: [
          // SOS Emergency Active Banner (Pulsating Red)
          if (meshService.isSosActive)
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
              color: Colors.red.shade900,
              child: Row(
                children: [
                  const Icon(Icons.warning, color: Colors.yellowAccent, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      '🚨 BROADCASTING EMERGENCY SOS (0.3s) • Packets Sent: ${meshService.sosBroadcastCount}',
                      style: const TextStyle(
                        color: Colors.yellowAccent,
                        fontWeight: FontWeight.bold,
                        fontSize: 12,
                      ),
                    ),
                  ),
                  TextButton(
                    onPressed: () => meshService.toggleSos(),
                    child: const Text('STOP', style: TextStyle(color: Colors.white, fontWeight: FontWeight.bold)),
                  ),
                ],
              ),
            )
          else
            // Standard Android BLE Mesh Status Indicator Banner
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
              color: Colors.cyan.shade900.withOpacity(0.4),
              child: Row(
                children: [
                  const Icon(Icons.bluetooth_searching, color: Colors.cyanAccent, size: 16),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'BLE Mesh Active • ${meshService.peers.length} Direct Peers Connected',
                      style: const TextStyle(color: Colors.cyanAccent, fontSize: 12),
                    ),
                  ),
                ],
              ),
            ),
          Expanded(
            child: meshService.messages.isEmpty
                ? const Center(
                    child: Text(
                      'No messages yet in [default]\nType a message, tap quick chips, or press SOS',
                      textAlign: TextAlign.center,
                      style: TextStyle(color: Colors.grey),
                    ),
                  )
                : ListView.builder(
                    controller: _scrollController,
                    padding: const EdgeInsets.all(12),
                    itemCount: meshService.messages.length,
                    itemBuilder: (context, index) {
                      final msg = meshService.messages[index];
                      return MessageBubble(message: msg);
                    },
                  ),
          ),
          // Quick Response Chips Bar
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
            child: Row(
              children: [
                ActionChip(
                  label: const Text('👍 I am Safe'),
                  onPressed: () {
                    meshService.sendMessage('👍 Status: I am safe');
                    _scrollToBottom();
                  },
                ),
                const SizedBox(width: 6),
                ActionChip(
                  label: const Text('💧 Need Water'),
                  onPressed: () {
                    meshService.sendMessage('💧 Status: Need drinking water');
                    _scrollToBottom();
                  },
                ),
                const SizedBox(width: 6),
                ActionChip(
                  label: const Text('🏥 Need Medical Aid'),
                  onPressed: () {
                    meshService.sendMessage('🏥 Status: Need medical assistance');
                    _scrollToBottom();
                  },
                ),
                const SizedBox(width: 6),
                ActionChip(
                  label: const Text('⛺ Safe Shelter'),
                  onPressed: () {
                    meshService.sendMessage('⛺ Status: Located safe shelter');
                    _scrollToBottom();
                  },
                ),
              ],
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
                    _scrollToBottom();
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
                      _scrollToBottom();
                    },
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.send, color: Colors.amber),
                  onPressed: () {
                    meshService.sendMessage(_textController.text);
                    _textController.clear();
                    _scrollToBottom();
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
