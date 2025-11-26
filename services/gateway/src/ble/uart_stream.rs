use btleplug::api::{Central, Characteristic, Manager as _, Peripheral as _, WriteType};
use btleplug::platform::Manager;
use bytes::BytesMut;
use futures::StreamExt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

const CHUNK_SZ: usize = 20;


const INTER_CHUNK_MS: u64 = 5;

pub struct BleUartStream {
    rx_buf: BytesMut,
    notif_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    periph: btleplug::platform::Peripheral,
    rx_char: Characteristic,
}

impl std::fmt::Debug for BleUartStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BleUartStream(buf={}b)", self.rx_buf.len())
    }
}

impl BleUartStream {.
    pub async fn connect(
        peripheral_id: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mgr = Manager::new().await?;

        let adapters = mgr.adapters().await?;
        let adap = adapters.into_iter().next()
            .ok_or("No Bluetooth adapter found")?;

        let periphs = adap.peripherals().await?;
        let periph = periphs.into_iter()
            .find(|p| p.id().to_string() == peripheral_id)
            .ok_or_else(|| format!("BLE device {peripheral_id} not in adapter cache - was a scan run?"))?;

        periph.connect().await?;
        periph.discover_services().await?;

        let chars = periph.characteristics();

        let rx_char = chars.iter()
            .find(|c| c.uuid == super::NUS_RX_UUID)
            .cloned()
            .ok_or("NUS RX char (0002) missing - wrong firmware?")?;

        let tx_char = chars.iter()
            .find(|c| c.uuid == super::NUS_TX_UUID)
            .cloned()
            .ok_or("NUS TX char (0003) missing - wrong firmware?")?;

        // subscribe to tcx
        periph.subscribe(&tx_char).await?;

        let (ntx, nrx) = mpsc::unbounded_channel();
        let mut notif_stream = periph.notifications().await?;

        // spawn a task to forward BLE notifications into our channel.
        // we filter by UUID because btleplug sends ALL notifications through
        // one stream, even from other characteristics if any exist.
        tokio::spawn(async move {
            while let Some(notif) = notif_stream.next().await {
                if notif.uuid == super::NUS_TX_UUID
                    && ntx.send(notif.value).is_err() {
                        // receiver dropped, stream is dead
                        break;
                    }
                // notifications from other characteristics just get dropped
            }
        });

        Ok(Self {
            rx_buf: BytesMut::new(),
            notif_rx: nrx,
            periph,
            rx_char,
        })
    }
}

//, AsyncRead: drain notifications into a buffer, hand bytes to caller ------

impl AsyncRead for BleUartStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // pull as many notifications as are available right now.
        // bLE notifications arrive in small chunks (typically 20 bytes each)
        // so we might need several to fill the caller's buffer.
        loop {
            match self.notif_rx.poll_recv(cx) {
                Poll::Ready(Some(data)) => {
                    self.rx_buf.extend_from_slice(&data);
                }
                Poll::Ready(None) => {
                    // channel closed = peripheral disconnected or task died
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "BLE notification channel closed - device disconnected?",
                    )));
                }
                Poll::Pending => break,
            }
        }

        if self.rx_buf.is_empty() {
            return Poll::Pending; 
        }

        let n = self.rx_buf.len().min(buf.remaining());
        let chunk = self.rx_buf.split_to(n);
        buf.put_slice(&chunk);
        Poll::Ready(Ok(()))
    }
}

//, AsyncWrite: chunk data to 20 bytes with inter-chunk delay ---------------

impl AsyncWrite for BleUartStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let p = self.periph.clone();
        let ch = self.rx_char.clone();
        let data = buf.to_vec();
        let total = data.len();

        tokio::spawn(async move {
            for (_i, chunk) in data.chunks(CHUNK_SZ).enumerate() {
                if p.write(&ch, chunk, WriteType::WithoutResponse).await.is_err() {
                    break;
                }
                if chunk.len() == CHUNK_SZ {
                    tokio::time::sleep(std::time::Duration::from_millis(INTER_CHUNK_MS)).await;
                }
            }
        });

        Poll::Ready(Ok(total))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // TODO: should we call peripheral.disconnect() here?
        Poll::Ready(Ok(()))
    }
}
