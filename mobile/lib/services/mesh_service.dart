import 'package:flutter/foundation.dart';
import 'package:geolocator/geolocator.dart';

enum MessageType { text, location }
enum MessageStatus { sending, relayed, delivered }

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
  final int hops;
  final MessageStatus status;

  ChatMessageModel({
    required this.senderId,
    required this.senderName,
    required this.text,
    this.type = MessageType.text,
    this.lat,
    this.lon,
    required this.timestamp,
    required this.isMe,
    this.hops = 1,
    this.status = MessageStatus.delivered,
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
      'hops': hops,
    };
  }
}

class MeshService extends ChangeNotifier {
  String _activeNetwork = "default";
  String _nodeId = "android-565c7b6a";
  final List<PeerNode> _peers = [];
  final List<ChatMessageModel> _messages = [];
  bool _isScanning = false;

  String get activeNetwork => _activeNetwork;
  String get nodeId => _nodeId;
  List<PeerNode> get peers => List.unmodifiable(_peers);
  List<ChatMessageModel> get messages => List.unmodifiable(_messages);
  bool get isScanning => _isScanning;

  Future<void> init() async {
    _isScanning = true;
    _peers.clear();
    _peers.add(PeerNode(
      id: "peer-a2fdb802",
      name: "Android-Peer-1",
      hops: 1,
      distanceMeters: 18.5,
      lat: -33.8688,
      lon: 151.2093,
      isDirect: true,
    ));
    _peers.add(PeerNode(
      id: "peer-c9103a4f",
      name: "Relay-Node-2",
      hops: 2,
      distanceMeters: 45.0,
      lat: -33.8692,
      lon: 151.2101,
      isDirect: false,
    ));
    notifyListeners();
  }

  void sendMessage(String text) {
    if (text.trim().isEmpty) return;
    final now = DateTime.now();
    final timeStr = "${now.hour.toString().padLeft(2, '0')}:${now.minute.toString().padLeft(2, '0')}";
    
    final newMsg = ChatMessageModel(
      senderId: _nodeId,
      senderName: "Android-Self",
      text: text,
      type: MessageType.text,
      timestamp: timeStr,
      isMe: true,
      hops: 1,
      status: MessageStatus.relayed,
    );

    _messages.add(newMsg);
    notifyListeners();

    // Simulate incoming P2P reply from mesh peer for testing
    Future.delayed(const Duration(seconds: 2), () {
      _messages.add(ChatMessageModel(
        senderId: "peer-a2fdb802",
        senderName: "Android-Peer-1",
        text: "Received: \"$text\" via BLE mesh (1 hop)",
        type: MessageType.text,
        timestamp: "${DateTime.now().hour.toString().padLeft(2, '0')}:${DateTime.now().minute.toString().padLeft(2, '0')}",
        isMe: false,
        hops: 1,
        status: MessageStatus.delivered,
      ));
      notifyListeners();
    });
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

    final now = DateTime.now();
    final timeStr = "${now.hour.toString().padLeft(2, '0')}:${now.minute.toString().padLeft(2, '0')}";

    _messages.add(ChatMessageModel(
      senderId: _nodeId,
      senderName: "Android-Self",
      text: "📍 Shared Emergency GPS Location",
      type: MessageType.location,
      lat: lat,
      lon: lon,
      timestamp: timeStr,
      isMe: true,
      hops: 1,
      status: MessageStatus.relayed,
    ));
    notifyListeners();
  }

  void createNetwork(String name) {
    _activeNetwork = name;
    notifyListeners();
  }
}
