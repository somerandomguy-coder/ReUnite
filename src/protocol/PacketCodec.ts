import { MeshMessage, MessageType } from '../contracts/mesh.js';

/**
 * Binary Packet Codec for Dev 2 (Mesh Protocol).
 * Packs MeshMessage objects into compact Uint8Array micro-frames (e.g. 30-40 bytes)
 * suitable for BLE Manufacturer Specific Data advertising packets.
 *
 * Micro-Frame Layout:
 * [0]     : Protocol Version (1 Byte)
 * [1]     : Message Type (0=CHAT, 1=SOS, 2=PING)
 * [2]     : Hops Remaining / TTL (1 Byte)
 * [3..6]  : Timestamp (4 Bytes, Big Endian Epoch Seconds)
 * [7..10] : Packet ID Checksum (4 Bytes)
 * [11..14]: Sender ID Hash (4 Bytes)
 * [15..]  : UTF-8 Payload Content
 */
export class PacketCodec {
  static VERSION = 1;

  static encode(msg: MeshMessage): Uint8Array {
    const textEncoder = new TextEncoder();
    const payloadBytes = textEncoder.encode(msg.content);
    
    // Header size = 15 bytes
    const buffer = new Uint8Array(15 + payloadBytes.length);
    const view = new DataView(buffer.buffer);

    buffer[0] = PacketCodec.VERSION;
    buffer[1] = msg.type === 'SOS' ? 1 : (msg.type === 'PING' ? 2 : 0);
    buffer[2] = msg.hopsRemaining;

    // Timestamp (epoch seconds)
    const epochSec = Math.floor(msg.timestamp / 1000);
    view.setUint32(3, epochSec, false); // Big-Endian

    // Numeric Packet ID hash (4 bytes)
    const msgIdHash = PacketCodec.hashString(msg.id);
    view.setUint32(7, msgIdHash, false);

    // Sender ID hash (4 bytes)
    const senderHash = PacketCodec.hashString(msg.senderId);
    view.setUint32(11, senderHash, false);

    // Payload text
    buffer.set(payloadBytes, 15);

    return buffer;
  }

  static decode(bytes: Uint8Array): MeshMessage | null {
    if (bytes.length < 15) return null;

    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);

    const version = bytes[0];
    if (version !== PacketCodec.VERSION) return null;

    const typeCode = bytes[1];
    const type: MessageType = typeCode === 1 ? 'SOS' : (typeCode === 2 ? 'PING' : 'CHAT');
    const hopsRemaining = bytes[2];
    const timestampSec = view.getUint32(3, false);
    const msgIdHash = view.getUint32(7, false);
    const senderHash = view.getUint32(11, false);

    const textDecoder = new TextDecoder('utf-8');
    const content = textDecoder.decode(bytes.subarray(15));

    return {
      id: `packet_${msgIdHash.toString(16)}`,
      senderId: `node_${senderHash.toString(16)}`,
      recipientId: 'BROADCAST',
      type,
      content,
      timestamp: timestampSec * 1000,
      hopsRemaining,
    };
  }

  private static hashString(str: string): number {
    let hash = 0x811c9dc5;
    for (let i = 0; i < str.length; i++) {
      hash ^= str.charCodeAt(i);
      hash += (hash << 1) + (hash << 4) + (hash << 7) + (hash << 8) + (hash << 24);
    }
    return hash >>> 0;
  }
}
