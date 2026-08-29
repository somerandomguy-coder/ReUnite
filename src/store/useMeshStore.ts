import { IMeshRouter, MeshMessage, DiscoveredNode } from '../contracts/mesh.js';

/**
 * Reactive State Store Wrapper (Dev 4).
 * Bridges the IMeshRouter contract into reactive UI state hooks.
 */
export class MeshStore {
  private router: IMeshRouter;
  private messages: MeshMessage[] = [];
  private nodes: DiscoveredNode[] = [];
  private listeners: Array<() => void> = [];

  constructor(router: IMeshRouter) {
    this.router = router;

    // Subscribe to mesh router events
    this.router.onMessageReceived((msg) => {
      this.messages.push(msg);
      this.nodes = this.router.getDiscoveredNodes();
      this.notifyUI();
    });
  }

  async sendEmergencySOS(content: string = '🚨 EMERGENCY SOS ALERT'): Promise<void> {
    await this.router.sendMessage({
      senderId: 'local_user',
      recipientId: 'BROADCAST',
      type: 'SOS',
      content,
    });
  }

  async sendChatMessage(content: string, recipientId: string = 'BROADCAST'): Promise<void> {
    await this.router.sendMessage({
      senderId: 'local_user',
      recipientId,
      type: 'CHAT',
      content,
    });
  }

  getMessages(): MeshMessage[] {
    return this.messages;
  }

  getDiscoveredNodes(): DiscoveredNode[] {
    return this.nodes;
  }

  subscribe(callback: () => void): () => void {
    this.listeners.push(callback);
    return () => {
      this.listeners = this.listeners.filter(l => l !== callback);
    };
  }

  private notifyUI() {
    this.listeners.forEach(cb => cb());
  }
}
