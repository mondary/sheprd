//! A connection whose first bytes were already read: they are read again
//! first, then the connection itself. The front reads a ClientHello to
//! route it, and a TLS server that answers there must see it whole.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// `inner`, with `prefix` read back ahead of it.
#[derive(Debug)]
pub struct Replay<S> {
    inner: S,
    prefix: Vec<u8>,
    read: usize,
}

impl<S> Replay<S> {
    #[must_use]
    pub fn new(prefix: Vec<u8>, inner: S) -> Self {
        Self {
            inner,
            prefix,
            read: 0,
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Replay<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let rest = &self.prefix[self.read..];
        if rest.is_empty() {
            return Pin::new(&mut self.inner).poll_read(context, buffer);
        }
        let count = rest.len().min(buffer.remaining());
        buffer.put_slice(&rest[..count]);
        self.read += count;
        Poll::Ready(Ok(()))
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Replay<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(context, bytes)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(context)
    }
}
