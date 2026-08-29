import { IStorageRepository, MeshMessage } from '../contracts/mesh.js';

/**
 * Local Storage & Cache Repository for Dev 4.
 * Handles local persistence (SQLite / AsyncStorage / LocalForage)
 * and Seen Packet Deduplication tracking.
 */
export class InMemoryStorageRepository implements IStorageRepository {
  private messages: MeshMessage[] = [];
  private seenPackets: Set<string> = new Set();

  async saveMessage(msg: MeshMessage): Promise<void> {
    this.messages.push(msg);
  }

  async getMessages(channelOrPeerId?: string): Promise<MeshMessage[]> {
    if (!channelOrPeerId || channelOrPeerId === 'BROADCAST') {
      return this.messages;
    }
    return this.messages.filter(m => m.senderId === channelOrPeerId || m.recipientId === channelOrPeerId);
  }

  recordSeenPacket(packetId: string): boolean {
    if (this.seenPackets.has(packetId)) {
      return false; // Duplicate
    }
    this.seenPackets.add(packetId);
    return true; // First time seen
  }
}
