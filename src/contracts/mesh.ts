/**
 * ============================================================================
 * REUNITE LOCAL-FIRST MESH ARCHITECTURE: SHARED CONTRACTS
 * ============================================================================
 * In a local-first mobile architecture, APIs are TypeScript Interfaces and
 * In-Memory Event Emitters rather than HTTP REST endpoints.
 *
 * This file is locked on Day 1 so all 4 developers can work in parallel
 * without blocking each other.
 */

// 1. Data Models
export type NodeType = 'victim' | 'saver' | 'relay';
export type MessageType = 'CHAT' | 'SOS' | 'PING';

export interface LocationCoordinates {
  lat: number;
  lng: number;
}

export interface MeshMessage {
  id: string;              // Unique 4-byte hash / UUID
  senderId: string;        // Node ID of original sender
  recipientId: string;     // "BROADCAST" or specific Node ID
  type: MessageType;
  content: string;
  timestamp: number;
  hopsRemaining: number;  // TTL: e.g. starts at 5, decrements at each relay
  location?: LocationCoordinates;
}

export interface DiscoveredNode {
  nodeId: string;
  role: NodeType;
  rssi: number;
  lastSeen: number;
  estimatedDistanceMeters?: number;
}

// 2. Hardware Driver Contract (Byte level: Dev 3)
export interface IRadioDriver {
  /** Transmit raw byte payload over connectionless BLE Extended Advertising / Beacon */
  broadcastPayload(bytes: Uint8Array): Promise<boolean>;
  
  /** Register callback for when raw bytes are scanned from nearby BLE devices */
  onPayloadDiscovered(callback: (bytes: Uint8Array, rssi: number) => void): void;
}

// 3. Routing Engine Contract (Logic level: Dev 2)
export interface IMeshRouter {
  /** Send a message into the mesh network */
  sendMessage(msg: Omit<MeshMessage, 'id' | 'timestamp' | 'hopsRemaining'>): Promise<void>;
  
  /** Subscribe to incoming messages (returns unsubscribe function) */
  onMessageReceived(callback: (msg: MeshMessage) => void): () => void;
  
  /** Retrieve currently discovered nearby mesh nodes */
  getDiscoveredNodes(): DiscoveredNode[];
}

// 4. Storage Contract (Database level: Dev 4)
export interface IStorageRepository {
  saveMessage(msg: MeshMessage): Promise<void>;
  getMessages(channelOrPeerId?: string): Promise<MeshMessage[]>;
  recordSeenPacket(packetId: string): boolean; // Returns false if duplicate
}
