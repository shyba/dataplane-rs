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

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
