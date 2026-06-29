pub type Result<T> = std::result::Result<T, NifError>;

#[derive(Debug, Clone)]
pub enum NifError {
    Errno(i32),
    Closed,
    Timeout,
    StartupProfileLayout,
}

impl NifError {
    pub fn from_errno(errno: i32) -> Self {
        NifError::Errno(errno)
    }

    pub fn last_os_error() -> Self {
        NifError::Errno(std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
    }

    pub fn atom_name(&self) -> &'static str {
        match self {
            NifError::Errno(e) => errno_to_atom(*e),
            NifError::Closed => "closed",
            NifError::Timeout => "timeout",
            NifError::StartupProfileLayout => "startup_profile_layout",
        }
    }
}

fn errno_to_atom(errno: i32) -> &'static str {
    match errno {
        libc::ECONNREFUSED => "econnrefused",
        libc::ECONNRESET => "econnreset",
        libc::ETIMEDOUT => "etimedout",
        libc::ENOBUFS => "enobufs",
        libc::ENOMEM => "enomem",
        libc::EPIPE => "epipe",
        libc::EBADF => "ebadf",
        libc::EINVAL => "einval",
        libc::ENOTCONN => "enotconn",
        libc::EADDRINUSE => "eaddrinuse",
        libc::EADDRNOTAVAIL => "eaddrnotavail",
        libc::ENETUNREACH => "enetunreach",
        libc::ENETDOWN => "enetdown",
        libc::EACCES => "eacces",
        libc::ENOENT => "enoent",
        libc::ENOSPC => "enospc",
        libc::ECANCELED => "ecanceled",
        libc::EOVERFLOW => "eoverflow",
        libc::EAGAIN => "eagain",
        _ => "eio",
    }
}
