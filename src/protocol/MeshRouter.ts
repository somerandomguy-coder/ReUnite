import { IMeshRouter, IRadioDriver, MeshMessage, DiscoveredNode } from '../contracts/mesh.js';
import { PacketCodec } from './PacketCodec.js';

/**
 * Pure TypeScript Mesh Routing Engine (Dev 2).
 * Implements A -> B -> C Epidemic Flooding with LRU Deduplication Cache
 * and TTL Hop Count Decrementing.
 *
 * Runs 100% in Node.js/Jest without needing physical mobile phones!
 */
export class MeshRouter implements IMeshRouter {
  public nodeId: string;
  public seenPacketCache: Set<string> = new Set();
  private radioDriver?: IRadioDriver;
  private messageListeners: Array<(msg: MeshMessage) => void> = [];
  private discoveredNodesMap: Map<string, DiscoveredNode> = new Map();

  /** Direct hook for in-memory virtual network simulation tests */
  public onRadioTransmit?: (bytes: Uint8Array) => void;

  constructor(nodeId: string, radioDriver?: IRadioDriver) {
    this.nodeId = nodeId;
    this.radioDriver = radioDriver;

    if (this.radioDriver) {
      this.radioDriver.onPayloadDiscovered((bytes, rssi) => {
        this.receiveRadioBytes(bytes, rssi);
      });
    }
  }

  async sendMessage(msg: Omit<MeshMessage, 'id' | 'timestamp' | 'hopsRemaining'>): Promise<void> {
    const fullMsg: MeshMessage = {
      ...msg,
      id: `msg_${Math.random().toString(36).substring(2, 9)}`,
      senderId: this.nodeId,
      timestamp: Date.now(),
      hopsRemaining: 5, // Initial TTL
    };

    // Mark our own message as seen so we don't process self-echoes
    this.seenPacketCache.add(fullMsg.id);

    // Encode to binary micro-frame
    const rawBytes = PacketCodec.encode(fullMsg);

    // Transmit via radio or virtual in-memory hook
    if (this.onRadioTransmit) {
      this.onRadioTransmit(rawBytes);
    }
    if (this.radioDriver) {
      await this.radioDriver.broadcastPayload(rawBytes);
    }
  }

  receiveRadioBytes(bytes: Uint8Array, rssi: number = -60): void {
    const msg = PacketCodec.decode(bytes);
    if (!msg) return;

    // 1. Deduplication Cache Check
    if (this.seenPacketCache.has(msg.id)) {
      // Duplicate packet already processed -> DROP SILENTLY (prevents infinite loops)
      return;
    }
    this.seenPacketCache.add(msg.id);

    // 2. Track Discovered Node
    this.discoveredNodesMap.set(msg.senderId, {
      nodeId: msg.senderId,
      role: msg.type === 'SOS' ? 'victim' : 'saver',
      rssi,
      lastSeen: Date.now(),
    });

    // 3. Notify Local UI Listeners
    this.messageListeners.forEach(cb => cb(msg));

    // 4. Epidemic Flooding & Hop Decrement
    if (msg.hopsRemaining > 1) {
      const relayedMsg: MeshMessage = {
        ...msg,
        hopsRemaining: msg.hopsRemaining - 1, // Decrement TTL
      };

      const relayedBytes = PacketCodec.encode(relayedMsg);

      // Re-broadcast payload to nearby ring of nodes
      if (this.onRadioTransmit) {
        this.onRadioTransmit(relayedBytes);
      }
      if (this.radioDriver) {
        this.radioDriver.broadcastPayload(relayedBytes).catch(err => {
          console.error('[MeshRouter] Relay broadcast error:', err);
        });
      }
    }
  }

  onMessageReceived(callback: (msg: MeshMessage) => void): () => void {
    this.messageListeners.push(callback);
    return () => {
      this.messageListeners = this.messageListeners.filter(l => l !== callback);
    };
  }

  getDiscoveredNodes(): DiscoveredNode[] {
    return Array.from(this.discoveredNodesMap.values());
  }
}
