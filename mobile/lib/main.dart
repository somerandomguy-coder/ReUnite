import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'app.dart';
import 'services/mesh_service.dart';

/// Peer addresses baked in at build time:
///
/// ```
/// flutter run --dart-define=MESH_PEERS=10.17.158.195:47474
/// ```
///
/// Comma-separated for more than one. This route exists because it is the only one
/// available before the app has ever run on the device - on iOS, where UDP broadcast and
/// multicast are entitlement-gated, a node with no seed has no way to find anybody. Once
/// the app is up, addresses typed into the Radio panel on the Networks tab are saved on
/// the device and merged in behind these.
const String kMeshPeers = String.fromEnvironment('MESH_PEERS');

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(
    ChangeNotifierProvider(
      create: (_) => MeshService()..init(peers: parsePeerList(kMeshPeers)),
      child: const ReUniteApp(),
    ),
  );
}
