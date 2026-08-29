import 'package:flutter/foundation.dart';

class PeerNode {
  final String id;
  final String name;
  final int hops;
  final double? distanceMeters;
  final bool isDirect;

  PeerNode({
    required this.id,
    required this.name,
    required this.hops,
    this.distanceMeters,
    required this.isDirect,
  });
}

class ChatMessageModel {
  final String senderId;
  final String senderName;
  final String text;
  final String timestamp;
  final bool isMe;

  ChatMessageModel({
    required this.senderId,
    required this.senderName,
    required this.text,
    required this.timestamp,
    required this.isMe,
  });
}

class MeshService extends ChangeNotifier {
  String _activeNetwork = "default";
  final String _nodeId = "565c7b6a6af53c06";
  final List<PeerNode> _peers = [];
  final List<ChatMessageModel> _messages = [];

  String get activeNetwork => _activeNetwork;
  String get nodeId => _nodeId;
  List<PeerNode> get peers => List.unmodifiable(_peers);
  List<ChatMessageModel> get messages => List.unmodifiable(_messages);

  Future<void> init() async {
    // TODO: Connect to native Rust meshcore via uniffi or flutter_rust_bridge
    _peers.add(PeerNode(
      id: "a2fdb80228bf2f0a",
      name: "macOS-Node",
      hops: 1,
      distanceMeters: 25.0,
      isDirect: true,
    ));
    notifyListeners();
  }

  void sendMessage(String text) {
    if (text.trim().isEmpty) return;
    _messages.add(ChatMessageModel(
      senderId: _nodeId,
      senderName: "Me",
      text: text,
      timestamp: DateTime.now().toIso8601String().substring(11, 16),
      isMe: true,
    ));
    notifyListeners();
  }

  void createNetwork(String name) {
    _activeNetwork = name;
    notifyListeners();
  }
}
