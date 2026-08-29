import asyncio
import sys
from bluez_peripheral.gatt.service import Service
from bluez_peripheral.gatt.characteristic import characteristic, CharacteristicFlags
from bluez_peripheral.advert import Advertisement, AdvertisingIncludes
from bluez_peripheral.util import Adapter, get_message_bus

SERVICE_UUID = "a1b2c3d4-e5f6-7890-1234-56789abcdef0"
RX_CHAR_UUID = "a1b2c3d4-e5f6-7890-1234-56789abcdef1"
TX_CHAR_UUID = "a1b2c3d4-e5f6-7890-1234-56789abcdef2"

class BitChatService(Service):
    def __init__(self):
        super().__init__(SERVICE_UUID, True)
        self._last_msg = b"Linux Node Online"

    @characteristic(RX_CHAR_UUID, CharacteristicFlags.WRITE | CharacteristicFlags.WRITE_WITHOUT_RESPONSE)
    def rx_char(self, options):
        return self._last_msg

    @rx_char.setter
    def rx_char_write(self, value, options):
        try:
            msg = value.decode("utf-8", errors="ignore")
            print(f"\n=======================================================")
            if "SOS" in msg.upper() or "EMERGENCY" in msg.upper():
                print(f"🚨🚨 [SOS EMERGENCY RECEIVED FROM PHONE]: {msg}")
            else:
                print(f"💬 [PHONE -> LINUX BLE]: {msg}")
            print("=======================================================")
            print("Linux Terminal Chat > ", end="", flush=True)
        except Exception as e:
            print(f"\n❌ Error decoding message: {e}")

    @characteristic(TX_CHAR_UUID, CharacteristicFlags.NOTIFY | CharacteristicFlags.READ)
    def tx_char(self, options):
        return self._last_msg

async def main():
    bus = await get_message_bus()
    adapter = await Adapter.get_first(bus)
    if not adapter:
        print("❌ No Bluetooth adapter found on Linux!")
        return

    service = BitChatService()
    await service.register(bus, adapter=adapter)

    advert = Advertisement(
        localName="BitChat-Linux",
        serviceUUIDs=[SERVICE_UUID],
        appearance=0,
        timeout=0,
        includes=AdvertisingIncludes.NONE
    )
    await advert.register(bus, adapter=adapter)

    print("=====================================================")
    print("📶 Starting Linux P2P BLE Chat & SOS Server...")
    print("=====================================================")
    print("✅ BLE GATT Service Registered!")
    print(f"   Service UUID: {SERVICE_UUID}")
    print("📡 Advertising as 'BitChat-Linux' over Bluetooth radio...")
    print("-----------------------------------------------------")
    print("Type a message below and press Enter to send over BLE to Phone:\n")

    loop = asyncio.get_running_loop()

    async def prompt_loop():
        while True:
            msg = await loop.run_in_executor(None, input, "Linux Terminal Chat > ")
            if msg.strip():
                payload = f"Linux: {msg}".encode("utf-8")
                service._last_msg = payload
                service.tx_char.changed(payload)
                print(f"📡 Sent over BLE Radio: '{msg}'")

    await prompt_loop()

if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n⏹️ Stopped BLE Chat Server.")
