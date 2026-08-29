import 'dart:io';

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'app.dart';
import 'services/mesh_service.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  // Bluetooth peripheral mode only exists on Android/iOS (see
  // MeshService._requestBluetoothPermissions); desktop platforms use the Wi-Fi/UDP
  // transport, same as the CLI.
  final transport =
      (Platform.isAndroid || Platform.isIOS) ? MeshTransport.bluetooth : MeshTransport.wifi;
  runApp(
    ChangeNotifierProvider(
      create: (_) => MeshService()..init(transport: transport),
      child: const ReUniteApp(),
    ),
  );
}
