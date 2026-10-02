use std::io;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Tokio stream wrapper used at the dataplane boundary.
///
/// It takes ownership of an already-accepted TCP fd and exposes
/// `AsyncRead`/`AsyncWrite` so higher layers (Hyper/Tokio services)
/// can run unchanged.
pub struct DataplaneTokioStream {
    inner: tokio::net::TcpStream,
}

impl DataplaneTokioStream {
    /// Builds a Tokio stream from an owned TCP socket descriptor.
    ///
    /// # Safety
    ///
    /// `fd` must be a valid, open TCP socket descriptor owned by the caller.
    /// After this call succeeds or fails, ownership is transferred and the
    /// caller must not close or otherwise use `fd` again.
    pub unsafe fn from_raw_fd(fd: RawFd) -> io::Result<Self> {
        // SAFETY: guaranteed by this function's caller. `from_raw_fd` takes
        // ownership of the descriptor, so all later error paths close through
        // `std_stream` drop.
        let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };
        Self::from_std(std_stream)
    }

    /// Wrap an owned TCP stream without requiring an unsafe descriptor transfer.
    ///
    /// Enables nonblocking I/O and TCP_NODELAY. Call within a Tokio runtime with
    /// I/O enabled, as required by `tokio::net::TcpStream::from_std`.
    pub fn from_std(std_stream: std::net::TcpStream) -> io::Result<Self> {
        std_stream.set_nonblocking(true)?;
        std_stream.set_nodelay(true)?;
        let inner = tokio::net::TcpStream::from_std(std_stream)?;
        Ok(Self { inner })
    }

    pub fn into_inner(self) -> tokio::net::TcpStream {
        self.inner
    }
}

impl AsRawFd for DataplaneTokioStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl AsyncRead for DataplaneTokioStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for DataplaneTokioStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn owned_stream_preserves_vectored_writes() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut stream = DataplaneTokioStream::from_std(client).unwrap();
            assert!(stream.is_write_vectored());
            assert!(stream.inner.nodelay().unwrap());
            let buffers = [io::IoSlice::new(b"hello"), io::IoSlice::new(b" world")];
            let count = stream.write_vectored(&buffers).await.unwrap();
            assert!(count > 0 && count <= 11);
            let mut received = vec![0; count];
            peer.read_exact(&mut received).unwrap();
            assert_eq!(received, b"hello world"[..count]);
        });
    }
}
