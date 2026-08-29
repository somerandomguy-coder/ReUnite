@TestOn('mac-os')
library;

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:reunite_mobile/services/mesh_service.dart';

/// Seed peers, end to end through the real Rust core.
///
/// Its own file because `mesh_start` binds a process-wide node and answers a second call
/// with "already running": a node started here inside `app_test.dart` would never get its
/// configuration as far as the transport, and the test would pass while proving nothing.
/// `flutter test` gives each file its own `flutter_tester` process, and so its own node.
///
/// Needs `./scripts/build_ffi.sh macos` first; skipped by the `@TestOn` guard elsewhere.
void main() {
  // Initialised because starting a node publishes a location, which reaches for the
  // geolocator plugin channel. There is none here; the binding just gives it something
  // to be refused by instead of an uninitialised-binding message.
  TestWidgetsFlutterBinding.ensureInitialized();

  test('a configured peer reaches the transport, and the core says so itself', () async {
    final home = Directory.systemTemp.createTempSync('reunite-peers-test');
    // Written before the first start: this is the on-disk half of the feature, the half
    // that has to survive the app being killed.
    File('${home.path}/peers.txt').writeAsStringSync('10.17.158.195:47474\n');

    final mesh = MeshService();
    await mesh.init(
      homeOverride: home.path,
      port: 47652,
      multicast: false,
      broadcast: false,
      name: 'peer-test',
      // The other route in: --dart-define=MESH_PEERS=192.168.1.42:47474
      peers: const ['192.168.1.42:47474'],
    );
    expect(mesh.startError, isNull,
        reason: 'run ./scripts/build_ffi.sh macos first — ${mesh.startError}');

    // Both routes, de-duplicated, build flag first.
    expect(mesh.seedPeers, ['192.168.1.42:47474', '10.17.158.195:47474']);

    // The core's own description of its transport is the only proof the seeds crossed the
    // FFI boundary rather than being collected in Dart and dropped there.
    final transport = mesh.me?.transport ?? '';
    expect(transport, contains('seeds [192.168.1.42:47474, 10.17.158.195:47474]'));
    // ...and the only proof the transport is not advertising reach it does not have,
    // which is what an iPhone would be doing with broadcast left on.
    expect(transport, isNot(contains('broadcast')));
    expect(transport, isNot(contains('multicast')));

    mesh.dispose();
    home.deleteSync(recursive: true);
  });
}
