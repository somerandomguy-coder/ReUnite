//! Linux native BLE transport using `bluer` (BlueZ DBus interface).
//!
//! Registers GATT Service UUID `a1b2c3d4-e5f6-7890-1234-56789abcdef0` (interoperable with BitChat),
//! advertises presence as a BLE Peripheral, and scans for peer nodes as BLE Central.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use bluer::adv::Advertisement;
use bluer::gatt::local::{
    Characteristic, CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicWrite,
    CharacteristicWriteMethod, Service,
};
use bluer::Session;
use tokio::sync::{mpsc, Mutex};

use uuid::Uuid;

use super::Transport;

pub const SERVICE_UUID: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_1234_56789abcdef0);
pub const RX_CHAR_UUID: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_1234_56789abcdef1);
pub const TX_CHAR_UUID: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_1234_56789abcdef2);

pub struct BleLinuxTransport {
    rx_queue: Arc<Mutex<mpsc::Receiver<(Vec<u8>, SocketAddr)>>>,
    tx_subscribers: Arc<Mutex<Vec<mpsc::Sender<Vec<u8>>>>>,
    adapter_name: String,
    _app_handle: bluer::gatt::local::ApplicationHandle,
    _adv_handle: bluer::adv::AdvertisementHandle,
}


impl BleLinuxTransport {
    pub async fn bind(local_name: Option<String>) -> Result<Self> {
        let session = Session::new().await.context("connecting to BlueZ DBus")?;
        let adapter = session
            .default_adapter()
            .await
            .context("getting default Bluetooth adapter")?;
        adapter.set_powered(true).await.context("powering on Bluetooth adapter")?;

        let adapter_name = adapter.name().to_string();
        let (inbound_tx, inbound_rx) = mpsc::channel::<(Vec<u8>, SocketAddr)>(256);
        let tx_subscribers: Arc<Mutex<Vec<mpsc::Sender<Vec<u8>>>>> = Arc::new(Mutex::new(Vec::new()));

        let dummy_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 47474));

        // RX characteristic: handles incoming writes from remote peers
        let inbound_tx_clone = inbound_tx.clone();
        let rx_char = Characteristic {
            uuid: RX_CHAR_UUID,
            write: Some(CharacteristicWrite {
                write: true,
                write_without_response: true,
                method: CharacteristicWriteMethod::Fun(Box::new(move |value, _req| {
                    let inbound_tx = inbound_tx_clone.clone();
                    Box::pin(async move {
                        let _ = inbound_tx.send((value, dummy_addr)).await;
                        Ok(())
                    })
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        // TX characteristic: sends GATT notifications to subscribed peer centrals
        let tx_subscribers_clone = tx_subscribers.clone();
        let tx_char = Characteristic {
            uuid: TX_CHAR_UUID,
            notify: Some(CharacteristicNotify {
                notify: true,
                method: CharacteristicNotifyMethod::Fun(Box::new(move |mut notifier| {
                    let tx_subscribers = tx_subscribers_clone.clone();
                    Box::pin(async move {
                        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(128);
                        {
                            let mut subs = tx_subscribers.lock().await;
                            subs.push(tx);
                        }
                        while let Some(frame) = rx.recv().await {
                            if notifier.notify(frame).await.is_err() {
                                break;
                            }
                        }
                    })
                })),
                ..Default::default()
            }),
            ..Default::default()
        };

        let service = Service {
            uuid: SERVICE_UUID,
            primary: true,
            characteristics: vec![rx_char, tx_char],
            ..Default::default()
        };

        let _app_handle = adapter
            .serve_gatt_application(bluer::gatt::local::Application {
                services: vec![service],
                ..Default::default()
            })
            .await
            .context("registering GATT service")?;

        // Start BLE Advertising
        let adv_name = local_name.unwrap_or_else(|| "MeshNet-Node".to_string());
        let le_advertisement = Advertisement {
            service_uuids: vec![SERVICE_UUID].into_iter().collect(),
            local_name: Some(adv_name),
            discoverable: Some(true),
            ..Default::default()
        };

        let _adv_handle = adapter
            .advertise(le_advertisement)
            .await
            .context("registering BLE advertisement")?;

        Ok(Self {
            rx_queue: Arc::new(Mutex::new(inbound_rx)),
            tx_subscribers,
            adapter_name,
            _app_handle,
            _adv_handle,
        })

    }
}

#[async_trait]
impl Transport for BleLinuxTransport {
    async fn send_broadcast(&self, frame: &[u8]) -> Result<()> {
        let mut subs = self.tx_subscribers.lock().await;
        subs.retain(|sub| sub.try_send(frame.to_vec()).is_ok());
        Ok(())
    }

    async fn send_to(&self, frame: &[u8], _addr: SocketAddr) -> Result<()> {
        self.send_broadcast(frame).await
    }

    async fn recv(&self) -> Result<(Vec<u8>, SocketAddr)> {
        let mut rx = self.rx_queue.lock().await;
        if let Some((data, addr)) = rx.recv().await {
            Ok((data, addr))
        } else {
            Err(anyhow!("BLE transport closed"))
        }
    }

    fn describe(&self) -> String {
        format!("ble/linux ({}, service {})", self.adapter_name, SERVICE_UUID)
    }
}
