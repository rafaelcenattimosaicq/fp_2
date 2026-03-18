
use crate::state::{SharedState, TrafficDirection};
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};


pub struct LoggedTransport<T> {
    inner: T,
    st: SharedState,
}

impl<T> LoggedTransport<T> {
    #[allow(clippy::missing_const_for_fn, reason = "Arc<RwLock<…>> prevents const")]
    pub fn new(inner: T, state: SharedState) -> Self {
        Self { inner, st: state }
    }
}

impl<T> fmt::Debug for LoggedTransport<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoggedTransport").finish_non_exhaustive()
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for LoggedTransport<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let x = self.get_mut();
        let n1 = buf.filled().len();
        let r = Pin::new(&mut x.inner).poll_read(cx, buf);

        // only log on successful read with actual data
        if matches!(&r, Poll::Ready(Ok(()))) {
            let n2 = buf.filled().len();
            if n2 > n1 {
                let v = buf.filled()[n1..n2].to_vec();
                if let Ok(mut s) = x.st.write() {
                    s.push_traffic(TrafficDirection::Rx, v);
                }
            }
        }
        r
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for LoggedTransport<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let x = self.get_mut();
        let r = Pin::new(&mut x.inner).poll_write(cx, buf);

        if let Poll::Ready(Ok(n)) = &r {
            if *n > 0 {
                let v = buf[..*n].to_vec();
                if let Ok(mut s) = x.st.write() { s.push_traffic(TrafficDirection::Tx, v) } else { /* poisoned lock, silently drop */ }
            }
        }
        r
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
