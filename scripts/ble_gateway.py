#!/usr/bin/env python3
"""
MeshNet BLE Gateway (P2P Bluetooth Low Energy Adapter)
Bridges local UDP meshnet traffic (127.0.0.1:47474) over BLE P2P radio.

Service UUID: a1b2c3d4-e5f6-7890-1234-56789abcdef0
RX Char UUID: a1b2c3d4-e5f6-7890-1234-56789abcdef1 (Write)
TX Char UUID: a1b2c3d4-e5f6-7890-1234-56789abcdef2 (Notify/Read)
"""

import asyncio
import argparse
import socket
import sys

SERVICE_UUID = "a1b2c3d4-e5f6-7890-1234-56789abcdef0"
RX_CHAR_UUID = "a1b2c3d4-e5f6-7890-1234-56789abcdef1"
TX_CHAR_UUID = "a1b2c3d4-e5f6-7890-1234-56789abcdef2"

class UdpBleBridge:
    def __init__(self, udp_port: int, mesh_port: int, node_name: str):
        self.udp_port = udp_port
        self.mesh_port = mesh_port
        self.node_name = node_name
        self.udp_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.udp_sock.setblocking(False)
        self.udp_sock.bind(("127.0.0.1", self.udp_port))
        self.ble_service = None
        self.loop = None

    async def start(self):
        self.loop = asyncio.get_running_loop()
        print("=" * 60)
        print(f"📡 MeshNet P2P BLE Gateway Active")
        print(f"   UDP Listening Port : 127.0.0.1:{self.udp_port}")
        print(f"   Target Mesh Port   : 127.0.0.1:{self.mesh_port}")
        print(f"   BLE Service UUID   : {SERVICE_UUID}")
        print("=" * 60)

        # Start listening for UDP frames from local meshnet node
        self.loop.create_task(self.listen_udp())

        # Start BLE Peripheral Advertising
        await self.start_ble_peripheral()

    async def listen_udp(self):
        """Read UDP frames sent by meshnet and broadcast them over BLE TX notification."""
        while True:
            try:
                data, _ = await self.loop.sock_recvfrom(self.udp_sock, 65535)
                if data:
                    print(f" [UDP -> BLE Radio]: Forwarding {len(data)} bytes over BLE...")
                    if self.ble_service:
                        self.ble_service.broadcast_tx(data)
            except Exception as e:
                print(f"⚠️ UDP Recv error: {e}")
                await asyncio.sleep(0.1)

    def forward_to_udp(self, raw_frame: bytes):
        """Receive BLE frame from remote peer and send to local meshnet via UDP."""
        try:
            print(f"📥 [BLE Radio -> UDP]: Received {len(raw_frame)} bytes over BLE, forwarding to meshnet...")
            self.udp_sock.sendto(raw_frame, ("127.0.0.1", self.mesh_port))
        except Exception as e:
            print(f"❌ Error forwarding BLE frame to UDP: {e}")

    async def start_ble_peripheral(self):
        try:
            from bluez_peripheral.gatt.service import Service
            from bluez_peripheral.gatt.characteristic import characteristic, CharacteristicFlags
            from bluez_peripheral.advert import Advertisement, AdvertisingIncludes
            from bluez_peripheral.util import Adapter, get_message_bus

            bridge = self

            class MeshNetService(Service):
                def __init__(self):
                    super().__init__(SERVICE_UUID, True)
                    self._last_val = b""

                @characteristic(RX_CHAR_UUID, CharacteristicFlags.WRITE | CharacteristicFlags.WRITE_WITHOUT_RESPONSE)
                def rx_char(self, options):
                    return self._last_val

                @rx_char.setter
                def rx_char_write(self, value, options):
                    bridge.forward_to_udp(value)

                @characteristic(TX_CHAR_UUID, CharacteristicFlags.NOTIFY | CharacteristicFlags.READ)
                def tx_char(self, options):
                    return self._last_val

                def broadcast_tx(self, payload: bytes):
                    self._last_val = payload
                    self.tx_char.changed(payload)

            bus = await get_message_bus()
            adapter = await Adapter.get_first(bus)
            if not adapter:
                print("❌ No Bluetooth adapter found on system!")
                return

            self.ble_service = MeshNetService()
            await self.ble_service.register(bus, adapter=adapter)

            advert = Advertisement(
                localName=self.node_name,
                serviceUUIDs=[SERVICE_UUID],
                appearance=0,
                timeout=0,
                includes=AdvertisingIncludes.NONE
            )
            await advert.register(bus, adapter=adapter)
            print(f"✅ BLE GATT Service registered & advertising as '{self.node_name}'!")

        except ImportError:
            print("⚠️ bluez-peripheral not installed or non-Linux system.")
            print("   For Linux: pip install bluez-peripheral bleak")
            print("   For macOS: Bleak Central client mode active.")
            await self.start_bleak_fallback()

    async def start_bleak_fallback(self):
        """Fallback scanner/central using Bleak for macOS/cross-platform discovery."""
        try:
            from bleak import BleakScanner

            print("🔎 Starting Bleak BLE P2P scanner...")
            discovered_peers = set()

            async def callback(device, advertising_data):
                if SERVICE_UUID.lower() in [u.lower() for u in advertising_data.service_uuids]:
                    if device.address not in discovered_peers:
                        discovered_peers.add(device.address)
                        print(f"✨ Discovered MeshNet BLE Peer: {device.name} [{device.address}]")

            scanner = BleakScanner(callback)
            await scanner.start()
        except Exception as e:
            print(f"⚠️ Bleak scanner error: {e}")

async def main():
    parser = argparse.ArgumentParser(description="MeshNet BLE P2P Gateway")
    parser.add_argument("--udp-port", type=int, default=47475, help="Port gateway listens on for meshnet UDP frames")
    parser.add_argument("--mesh-port", type=int, default=47474, help="Port local meshnet node is listening on")
    parser.add_argument("--name", type=str, default="MeshNet-BLE", help="BLE local advertisement name")
    args = parser.parse_args()

    bridge = UdpBleBridge(args.udp_port, args.mesh_port, args.name)
    await bridge.start()

    await asyncio.Event().wait()

if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n⏹️ Stopped BLE Gateway.")
