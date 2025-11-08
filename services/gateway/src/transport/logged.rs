use crate::state::{SharedState, TrafficDirection};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct LoggedTransport<T> {
    inner: T,
    st: SharedState,
}

impl<T> std::fmt::Debug for LoggedTransport<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoggedTransport").finish()
    }
}

impl<T> LoggedTransport<T> {
    pub fn new(inner: T, state: SharedState) -> Self {
        Self { inner, st: state }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for LoggedTransport<T> {
    fn poll_read(
        mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let filled = &buf.filled()[before..];
            if !filled.is_empty() {
                if let Ok(mut s) = self.st.write() {
                    s.push_traffic(TrafficDirection::Rx, filled.to_vec());
                }
            }
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for LoggedTransport<T> {
    fn poll_write(
        mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &result {
            if let Ok(mut s) = self.st.write() {
                s.push_traffic(TrafficDirection::Tx, buf[..*n].to_vec());
            }
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
