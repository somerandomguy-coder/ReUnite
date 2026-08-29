import { IMeshRouter, MeshMessage, DiscoveredNode } from '../contracts/mesh.js';

/**
 * Mock Mesh Router for Frontend UI Developer (Dev 1).
 * Emits simulated incoming SOS messages and radar node blips on timers.
 * Allows building the entire React Native / Web UI without touching BLE hardware!
 */
export class MockMeshRouter implements IMeshRouter {
  private listeners: Array<(msg: MeshMessage) => void> = [];
  private mockNodes: DiscoveredNode[] = [
    { nodeId: 'node_rescuer_alpha', role: 'saver', rssi: -58, lastSeen: Date.now(), estimatedDistanceMeters: 12 },
    { nodeId: 'node_victim_bravo', role: 'victim', rssi: -74, lastSeen: Date.now(), estimatedDistanceMeters: 35 },
  ];

  constructor() {
    // Simulate an incoming emergency SOS broadcast 4 seconds after startup
    setTimeout(() => {
      this.emitMockIncomingMessage({
        id: `sos_${Date.now()}`,
        senderId: 'node_victim_bravo',
        recipientId: 'BROADCAST',
        type: 'SOS',
        content: '🚨 Structural Collapse at Sector 4! Need assistance!',
        timestamp: Date.now(),
        hopsRemaining: 4,
        location: { lat: 37.7749, lng: -122.4194 }
      });
    }, 4000);
  }

  async sendMessage(msg: Omit<MeshMessage, 'id' | 'timestamp' | 'hopsRemaining'>): Promise<void> {
    const fullMsg: MeshMessage = {
      ...msg,
      id: `msg_${Math.floor(Math.random() * 1000000)}`,
      timestamp: Date.now(),
      hopsRemaining: 5,
    };
    console.log('[MockMeshRouter] Outbound message queued:', fullMsg);
  }

  onMessageReceived(callback: (msg: MeshMessage) => void): () => void {
    this.listeners.push(callback);
    return () => {
      this.listeners = this.listeners.filter(l => l !== callback);
    };
  }

  getDiscoveredNodes(): DiscoveredNode[] {
    return this.mockNodes;
  }

  private emitMockIncomingMessage(msg: MeshMessage) {
    this.listeners.forEach(cb => cb(msg));
  }
}
