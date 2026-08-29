@TestOn('mac-os')
library;

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:provider/provider.dart';
import 'package:reunite_mobile/app.dart';
import 'package:reunite_mobile/services/mesh_service.dart';

/// End-to-end: the real widgets driven by the real Rust core.
///
/// `flutter test` runs on the host VM, where `dart:ffi` works, so this loads the same
/// libmeshffi the shipped app loads and starts a genuine mesh node. It is the check that
/// the UI is actually wired to the core rather than to a mock - which is what the whole
/// of Phase 2 step 2.1 is about.
///
/// Needs `./scripts/build_ffi.sh macos` first; skipped by the `@TestOn` guard elsewhere.
void main() {
  late Directory home;
  late MeshService mesh;

  setUpAll(() async {
    home = Directory.systemTemp.createTempSync('reunite-ui-test');
    mesh = MeshService();
    // An unusual port and no discovery, so this never joins a real mesh on the machine.
    await mesh.init(
      homeOverride: home.path,
      port: 47651,
      multicast: false,
      broadcast: false,
      name: 'test-node',
    );
  });

  tearDownAll(() {
    mesh.dispose();
    if (home.existsSync()) home.deleteSync(recursive: true);
  });

  Future<void> pumpApp(WidgetTester tester) async {
    await tester.pumpWidget(
      ChangeNotifierProvider<MeshService>.value(
        value: mesh,
        child: const ReUniteApp(),
      ),
    );
    await tester.pump(const Duration(milliseconds: 300));
  }

  testWidgets('the core starts and the app gets past the startup gate', (tester) async {
    expect(mesh.startError, isNull,
        reason: 'run ./scripts/build_ffi.sh macos first — ${mesh.startError}');
    expect(mesh.started, isTrue);
    expect(mesh.nodeId.length, 16, reason: 'a real hashed node id, not a placeholder');

    await pumpApp(tester);
    expect(find.text('starting the mesh...'), findsNothing);
    expect(find.text('The mesh core did not start'), findsNothing);
    // Chat is the landing screen and shows the active network in its title.
    expect(find.text('[default]'), findsOneWidget);
  });

  testWidgets('the panic buttons come from the core, not from hard-coded Dart',
      (tester) async {
    await pumpApp(tester);
    await tester.tap(find.text('Emergency'));
    await tester.pumpAndSettle();

    expect(mesh.statusCodes.length, 7, reason: 'seven pre-canned codes live in Rust');
    expect(find.text('Need medical help'), findsOneWidget);
    expect(find.text('Trapped - need rescue'), findsOneWidget);
    // The isolation notice is a requirement, not decoration.
    expect(find.textContaining('does NOT call emergency services'), findsOneWidget);
    expect(find.text('Slide to broadcast SOS'), findsOneWidget);
  });

  testWidgets('tapping a panic button really sends one byte through the core',
      (tester) async {
    await pumpApp(tester);
    await tester.tap(find.text('Emergency'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Need medical help'));
    await tester.pumpAndSettle();

    // The core is the source of truth, not local widget state.
    expect(mesh.myStatus, 2);
    expect(mesh.me?.status, 2);
  });

  testWidgets('SOS raises in the core and the banner appears', (tester) async {
    expect(mesh.setSos(true), isNull);
    await pumpApp(tester);
    expect(mesh.sosActive, isTrue);
    expect(find.textContaining('YOUR SOS IS ACTIVE'), findsOneWidget);

    expect(mesh.setSos(false), isNull);
    await pumpApp(tester);
    expect(mesh.sosActive, isFalse);
  });

  testWidgets('a reported zone reaches the heat map with its consensus count',
      (tester) async {
    expect(mesh.setLocation(10.7769, 106.7009), isNull);
    expect(mesh.reportZone(10.7769, 106.7009, 4), isNull);

    await pumpApp(tester);
    await tester.tap(find.text('Emergency'));
    await tester.pumpAndSettle();

    expect(mesh.zones, isNotEmpty);
    expect(mesh.zones.first.consensus, 1);
    expect(mesh.zones.first.mine, isTrue);

    // The heat map is the last section of the screen, so scroll it into view before
    // asserting on it - a lazy ListView has not built what is still off-screen.
    for (var i = 0; i < 6 && find.text('4.0/4').evaluate().isEmpty; i++) {
      await tester.drag(find.byType(ListView).first, const Offset(0, -400));
      await tester.pumpAndSettle();
    }

    // One report is deliberately labelled unverified rather than shown as agreement.
    expect(find.text('1 report — unverified'), findsWidgets);
    expect(find.text('4.0/4'), findsWidgets);
  });

  testWidgets('creating a private network shows it in the Networks tab', (tester) async {
    expect(mesh.createNetwork('rescue'), isNull);
    await pumpApp(tester);
    await tester.tap(find.text('Networks'));
    await tester.pumpAndSettle();

    expect(find.text('rescue'), findsOneWidget);
    expect(find.text('default'), findsOneWidget);
    expect(find.textContaining('Treat anything here as public'), findsOneWidget);
    // Our own node id must be shown so someone can be invited by it.
    expect(find.text(mesh.nodeId), findsOneWidget);
  });

  testWidgets('the peers tab degrades to compass mode with no map tiles', (tester) async {
    await pumpApp(tester);
    await tester.tap(find.text('Peers'));
    await tester.pumpAndSettle();

    expect(find.textContaining('Compass mode'), findsOneWidget);
    // Nothing else is on this mesh, and that must read as an explanation not an error.
    expect(find.textContaining('No peers heard yet'), findsOneWidget);
  });
}
