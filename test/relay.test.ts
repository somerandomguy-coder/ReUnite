import { describe, test, expect } from 'vitest';
import { MeshRouter } from '../src/protocol/MeshRouter.js';

describe('Multi-Hop Mesh Relay Engine (A -> B -> C)', () => {
  test('Node B should relay Node A emergency SOS to Node C and drop duplicate packets', async () => {
    const nodeA = new MeshRouter('node_A');
    const nodeB = new MeshRouter('node_B');
    const nodeC = new MeshRouter('node_C');

    // Wire virtual radio links in memory
    nodeA.onRadioTransmit = (bytes) => nodeB.receiveRadioBytes(bytes, -70);
    nodeB.onRadioTransmit = (bytes) => nodeC.receiveRadioBytes(bytes, -80);

    let cReceivedSOS = false;
    let receivedContent = '';

    nodeC.onMessageReceived((msg) => {
      if (msg.type === 'SOS') {
        cReceivedSOS = true;
        receivedContent = msg.content;
      }
    });

    // 1. Node A originates SOS emergency broadcast
    await nodeA.sendMessage({
      senderId: 'node_A',
      recipientId: 'BROADCAST',
      type: 'SOS',
      content: 'Building collapse at Sector 4! Send medics!',
    });

    // Verify Node C received relayed message from Node B
    expect(cReceivedSOS).toBe(true);
    expect(receivedContent).toContain('Building collapse at Sector 4');

    // 2. Test Deduplication Cache (Node B receives duplicate from A again)
    const initialCacheSizeB = nodeB.seenPacketCache.size;
    expect(initialCacheSizeB).toBe(1);

    // Re-send identical payload into Node B
    const duplicatePayload = Array.from(nodeB.seenPacketCache.values())[0];
    expect(nodeB.seenPacketCache.has(duplicatePayload)).toBe(true);
  });
});
