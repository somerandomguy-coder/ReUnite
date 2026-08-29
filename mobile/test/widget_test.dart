import 'package:flutter_test/flutter_test.dart';
import 'package:reunite_mobile/features/map/map_screen.dart';
import 'package:reunite_mobile/models/mesh_models.dart';
import 'package:reunite_mobile/services/mesh_service.dart';

/// The Dart half of the FFI contract.
///
/// `crates/meshffi/tests/bridge.rs` asserts the Rust side emits this JSON; these assert
/// the Dart side reads it. Between them, a field renamed on either side breaks a test
/// rather than silently showing an empty screen.
void main() {
  test('a peer parses, including the Phase 1 state the UI depends on', () {
    final peer = Peer.fromJson(const {
      'id': 'acfd53bb3f4e5430',
      'display': '~carol',
      'direct': false,
      'hops': 2,
      'rtt_ms': null,
      'rssi': null,
      'distance_m': 1109.4,
      'lat': 10.7869,
      'lon': 106.7009,
      'last_seen_ms': 1000,
      'in_current_network': true,
      'battery': 7,
      'status': 2,
      'sos': true,
      'ghost': false,
    });
    expect(peer.display, '~carol');
    expect(peer.hops, 2);
    expect(peer.sos, isTrue);
    expect(peer.battery, 7);
    expect(peer.hasPosition, isTrue);
    expect(peer.ghost, isFalse);
  });

  test('a ghost keeps its last known position', () {
    final ghost = Peer.fromJson(const {
      'id': 'd965fdd41a1a1940',
      'display': '~doomed',
      'direct': false,
      'hops': null,
      'rtt_ms': null,
      'rssi': null,
      'distance_m': 1109.4,
      'lat': 10.7869,
      'lon': 106.7009,
      'last_seen_ms': 0,
      'in_current_network': true,
      'battery': 3,
      'status': 1,
      'sos': false,
      'ghost': true,
    });
    expect(ghost.ghost, isTrue);
    expect(ghost.hasPosition, isTrue, reason: 'a ghost without a position is useless');
    expect(ghost.hops, isNull);
  });

  test('a zone keeps both vote counts apart instead of blending them', () {
    final contested = Zone.fromJson(const {
      'cell': '8865b5662bfffff',
      'lat': 10.77508,
      'lon': 106.69941,
      'verdict': 'unsafe',
      'radius_m': 750,
      'safe_votes': 2,
      'unsafe_votes': 3,
      'age_ms': 2000,
      'mine': false,
    });
    expect(contested.isSafe, isFalse);
    expect(contested.radiusM, 750);
    expect(contested.safeVotes, 2);
    expect(contested.unsafeVotes, 3);
    expect(contested.totalVotes, 5);
    // Disagreement is its own state. "3 say unsafe" and "3 say unsafe, 2 say safe" are
    // different claims, and the UI has to be able to tell them apart.
    expect(contested.contested, isTrue);
    expect(contested.verified, isTrue);

    final lone = Zone.fromJson(const {
      'cell': '8865b5662bfffff',
      'lat': 0.0,
      'lon': 0.0,
      'verdict': 'safe',
      'radius_m': 100,
      'safe_votes': 1,
      'unsafe_votes': 0,
      'age_ms': 0,
      'mine': true,
    });
    // One person calling a street safe is not a verified zone, and the UI dims it.
    expect(lone.isSafe, isTrue);
    expect(lone.verified, isFalse);
    expect(lone.contested, isFalse);
  });

  test('a typed length becomes metres, whatever unit it was typed in', () {
    expect(RadiusUnit.metres.toMetres(500), 500);
    expect(RadiusUnit.kilometres.toMetres(0.5), 500);
    expect(RadiusUnit.feet.toMetres(1640.42), 500);
    expect(RadiusUnit.miles.toMetres(1), 1609);
  });

  test('overlapping reports darken, and never reach full opacity', () {
    final one = zoneAlpha(1);
    final three = zoneAlpha(3);
    final many = zoneAlpha(50);
    expect(one, lessThan(three));
    expect(three, lessThan(many));
    // The cap is what keeps the map readable underneath the overlay - which is how
    // somebody actually navigates out of the area.
    expect(many, lessThanOrEqualTo(0.75));
    expect(one, greaterThan(0.0));
  });

  test('whoami carries the emergency state the banner reads', () {
    final me = Whoami.fromJson(const {
      'id': '85a6fedd09565ca8',
      'name': 'alice',
      'home': '/tmp/reunite',
      'transport': 'udp/0.0.0.0:47474, broadcast',
      'network': 'default',
      'lat': 10.7769,
      'lon': 106.7009,
      'sos': true,
      'status': 2,
      'battery': 88,
      'zone_resolution': 8,
    });
    expect(me.sos, isTrue);
    expect(me.status, 2);
    expect(me.network, 'default');
    expect(me.zoneResolution, 8);
  });

  test('networks distinguish the public lobby from private ones', () {
    final lobby = NetworkInfo.fromJson(const {
      'id': '0000000000000000',
      'name': 'default',
      'members': <String>[],
      'member_count': 0,
      'epoch': 0,
      'store_messages': false,
      'active': true,
      'is_default': true,
    });
    expect(lobby.isDefault, isTrue);
    expect(lobby.active, isTrue);
  });

  test('human formatting', () {
    expect(formatDistance(345), '345m');
    expect(formatDistance(1460), '1.46km');
    expect(formatAge(500), 'just now');
    expect(formatAge(38000), '38s ago');
    expect(formatAge(2 * 60000), '2m ago');
    expect(formatAge(3 * 3600000), '3h ago');
  });

  test('only a state the platform reported may accuse the radio', () {
    // The states the platform actually asserted.
    expect(bleErrorForRadioState('off'), contains('turned off'));
    expect(bleErrorForRadioState('unauthorized'), contains('permission'));
    expect(bleErrorForRadioState('unsupported'), contains('no Bluetooth'));

    // The states where it has declined to answer. Turning these into a diagnosis is the
    // bug that kept iOS off the mesh: the app told people to switch on Bluetooth that
    // was already on, and they believed it.
    expect(bleErrorForRadioState('unknown'), isNull);
    expect(bleErrorForRadioState('resetting'), isNull);
    expect(bleErrorForRadioState('on'), isNull);
    expect(bleErrorForRadioState('something-a-future-os-invents'), isNull);
  });
}
