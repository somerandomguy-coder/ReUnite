import 'package:flutter/foundation.dart';
import 'package:geolocator/geolocator.dart';

enum MessageType { text, location }

class PeerNode {
  final String id;
  final String name;
  final int hops;
  final double? distanceMeters;
  final double? lat;
  final double? lon;
  final bool isDirect;

  PeerNode({
    required this.id,
    required this.name,
    required this.hops,
    this.distanceMeters,
    this.lat,
    this.lon,
    required this.isDirect,
  });
}

class ChatMessageModel {
  final String senderId;
  final String senderName;
  final String text;
  final MessageType type;
  final double? lat;
  final double? lon;
  final String timestamp;
  final bool isMe;

  ChatMessageModel({
    required this.senderId,
    required this.senderName,
    required this.text,
    this.type = MessageType.text,
    this.lat,
    this.lon,
    required this.timestamp,
    required this.isMe,
  });

  /// Export as JSON payload string for mesh broadcast
  Map<String, dynamic> toJson() {
    return {
      'senderId': senderId,
      'senderName': senderName,
      'text': text,
      'type': type.name,
      'lat': lat,
      'lon': lon,
      'timestamp': timestamp,
    };
  }
}

class MeshService extends ChangeNotifier {
  String _activeNetwork = "default";
  String _nodeId = "565c7b6a6af53c06";
  final List<PeerNode> _peers = [];
  final List<ChatMessageModel> _messages = [];

  String get activeNetwork => _activeNetwork;
  String get nodeId => _nodeId;
  List<PeerNode> get peers => List.unmodifiable(_peers);
  List<ChatMessageModel> get messages => List.unmodifiable(_messages);

  Future<void> init() async {
    _peers.add(PeerNode(
      id: "a2fdb80228bf2f0a",
      name: "macOS-Node",
      hops: 1,
      distanceMeters: 25.0,
      lat: -33.8688,
      lon: 151.2093,
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
      type: MessageType.text,
      timestamp: DateTime.now().toIso8601String().substring(11, 16),
      isMe: true,
    ));
    notifyListeners();
  }

  Future<void> shareCurrentLocation() async {
    double lat = -33.8688; // Default emergency fallback coords
    double lon = 151.2093;

    try {
      bool serviceEnabled = await Geolocator.isLocationServiceEnabled();
      if (serviceEnabled) {
        LocationPermission permission = await Geolocator.checkPermission();
        if (permission == LocationPermission.denied) {
          permission = await Geolocator.requestPermission();
        }
        if (permission == LocationPermission.whileInUse ||
            permission == LocationPermission.always) {
          Position pos = await Geolocator.getCurrentPosition(
            desiredAccuracy: LocationAccuracy.high,
          );
          lat = pos.latitude;
          lon = pos.longitude;
        }
      }
    } catch (e) {
      debugPrint("Offline GPS fetch warning: $e");
    }

    _messages.add(ChatMessageModel(
      senderId: _nodeId,
      senderName: "Me",
      text: "📍 Shared Emergency GPS Location",
      type: MessageType.location,
      lat: lat,
      lon: lon,
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
