import { IRadioDriver } from '../contracts/mesh.js';

/**
 * BLE Hardware Radio Driver Specification & Blueprint (Dev 3).
 *
 * WHY MOVE FROM 1-TO-1 GATT PAIRING TO CONNECTIONLESS ADVERTISING:
 * -----------------------------------------------------------------
 * 1-to-1 GATT connections (pairing/connecting) do not scale for multi-hop mesh
 * because mobile operating systems (iOS / Android) limit active simultaneous
 * GATT connections to ~3-7 devices.
 *
 * CONNECTIONLESS ADVERTISING (BEACON BROADCAST):
 * -----------------------------------------------------------------
 * 1. Node A packs a 30-byte micro-frame into the Manufacturer Data field of a BLE Advertising packet.
 * 2. Node A advertises this packet for ~300-800 ms.
 * 3. Every nearby device scanning in the background picks up the raw advertisement,
 *    extracts the payload bytes, passes them to MeshRouter to check deduplication,
 *    decrements TTL (hopsRemaining), and re-advertises to the next ring of nodes.
 */

export class MockBleRadioDriver implements IRadioDriver {
  private payloadCallbacks: Array<(bytes: Uint8Array, rssi: number) => void> = [];

  async broadcastPayload(bytes: Uint8Array): Promise<boolean> {
    console.log(`[BleRadioDriver] Broadcasting ${bytes.length} bytes over BLE Manufacturer Data beacon...`);
    return true;
  }

  onPayloadDiscovered(callback: (bytes: Uint8Array, rssi: number) => void): void {
    this.payloadCallbacks.push(callback);
  }

  /** Simulate receiving a raw BLE payload from a physical hardware scanner */
  simulateIncomingRadioPacket(bytes: Uint8Array, rssi: number = -65): void {
    this.payloadCallbacks.forEach(cb => cb(bytes, rssi));
  }
}
