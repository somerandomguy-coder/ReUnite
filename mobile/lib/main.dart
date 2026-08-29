import 'dart:io';

import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

import 'app.dart';
import 'services/mesh_service.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  // Android/iOS advertise and scan; macOS scans only (see BleMeshCentral.swift) but can
  // still reach a phone with no network. Other desktop platforms use the Wi-Fi/UDP
  // transport, same as the CLI.
  final transport = (Platform.isAndroid || Platform.isIOS || Platform.isMacOS)
      ? MeshTransport.bluetooth
      : MeshTransport.wifi;
  runApp(
    ChangeNotifierProvider(
      create: (_) => MeshService()..init(transport: transport),
      child: const ReUniteApp(),
    ),
  );
}
