//! Startup profile dispatch and io_uring feature probe helpers.
//!
//! Extracted from runtime.rs to reduce monolith size and improve locality
//! of startup-related logic.

use crate::errors::NifError;
use crate::runtime_reactor::SqpollConfig;
#[cfg(not(feature = "exec-strategy-sqpoll"))]
use crate::runtime_reactor::SqpollMode;
use dataplane_core_reactor::balanced_profile::BalancedProfileLayout;
use dataplane_runtime::errors::Result;
use dataplane_runtime::runtime_profiles::TopologyProfile;

const ERRNO_FEAT_NOT_SUPPORTED: i32 = libc::EOPNOTSUPP;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeProfileDispatchSeam {
    Balanced,
    Embedded,
    Performance,
}

pub(super) fn dispatch_runtime_startup_profile_layout(
    profile: TopologyProfile,
) -> Result<(BalancedProfileLayout, RuntimeProfileDispatchSeam)> {
    use dataplane_runtime::runtime_profiles::dispatch_profile_layout_from_profile;
    dispatch_profile_layout_from_profile(
        profile,
        |layout| (layout, RuntimeProfileDispatchSeam::Balanced),
        |layout| (layout.inner().clone(), RuntimeProfileDispatchSeam::Embedded),
        |layout| {
            (
                layout.inner().clone(),
                RuntimeProfileDispatchSeam::Performance,
            )
        },
    )
    .map_err(|_| NifError::StartupProfileLayout)
}

pub(super) fn map_probe_err(err: std::io::Error) -> NifError {
    let errno = err.raw_os_error().unwrap_or(libc::EIO);
    match errno {
        libc::ENOSYS | libc::EINVAL | libc::EOPNOTSUPP => {
            NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED)
        }
        _ => NifError::from_errno(errno),
    }
}

pub(super) fn map_feature_error(err: std::io::Error) -> NifError {
    let errno = err.raw_os_error().unwrap_or(libc::EIO);
    match errno {
        libc::ENOSYS | libc::EOPNOTSUPP => NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED),
        _ => NifError::from_errno(errno),
    }
}

use io_uring::{opcode, IoUring, Probe};

pub(super) fn probe_required_runtime_features(
    ring: &IoUring,
    require_fixed_buffers: bool,
    require_provided_buffers: bool,
) -> Result<()> {
    let mut probe = Probe::new();
    ring.submitter()
        .register_probe(&mut probe)
        .map_err(map_probe_err)?;

    if !probe.is_supported(opcode::Accept::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    if !probe.is_supported(opcode::Recv::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    if !probe.is_supported(opcode::Read::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    if !probe.is_supported(opcode::Send::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    if !probe.is_supported(opcode::Writev::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    if require_fixed_buffers && !probe.is_supported(opcode::ReadFixed::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    if require_provided_buffers && !probe.is_supported(opcode::ProvideBuffers::CODE) {
        return Err(NifError::from_errno(ERRNO_FEAT_NOT_SUPPORTED));
    }

    Ok(())
}

/// Returns true if the given errno is a recoverable SQPOLL fallback error.
#[cfg(not(feature = "exec-strategy-sqpoll"))]
fn is_sqpoll_fallback_errno(errno: i32) -> bool {
    matches!(
        errno,
        libc::EPERM | libc::EINVAL | libc::ENOSYS | libc::EOPNOTSUPP
    )
}

/// Build an io_uring ring with optional SQPOLL support (non-SQPOLL variant).
///
/// This variant supports fallback to a plain ring when SQPOLL cannot be
/// established and the mode is `SqpollMode::Try`.
#[cfg(not(feature = "exec-strategy-sqpoll"))]
pub(crate) fn build_ring(
    entries: u32,
    sqpoll_config: SqpollConfig,
    sqpoll_cpu: Option<usize>,
) -> Result<IoUring> {
    let build_plain = || {
        IoUring::builder()
            .build(entries)
            .map_err(|e| NifError::from_errno(e.raw_os_error().unwrap_or(libc::EIO)))
    };

    if matches!(sqpoll_config.mode, SqpollMode::Off) {
        return build_plain();
    }

    let mut builder = IoUring::builder();
    builder.setup_clamp();
    builder.setup_sqpoll(sqpoll_config.idle_ms);
    builder.setup_single_issuer();
    if let Some(cpu) = sqpoll_cpu {
        builder.setup_sqpoll_cpu(cpu as u32);
    }

    match builder.build(entries) {
        Ok(ring) => {
            if ring.params().is_feature_sqpoll_nonfixed() {
                Ok(ring)
            } else if matches!(sqpoll_config.mode, SqpollMode::Try) {
                build_plain()
            } else {
                Err(NifError::from_errno(libc::EOPNOTSUPP))
            }
        }
        Err(err) => {
            let errno = err.raw_os_error().unwrap_or(libc::EIO);
            if matches!(sqpoll_config.mode, SqpollMode::Try) && is_sqpoll_fallback_errno(errno) {
                build_plain()
            } else {
                Err(NifError::from_errno(errno))
            }
        }
    }
}

/// Build an io_uring ring with SQPOLL required (SQPOLL variant).
///
/// Unlike the non-SQPOLL variant, this does not fall back to a plain ring on
/// failure. If SQPOLL cannot be established or the feature is unsupported,
/// an error is returned.
#[cfg(feature = "exec-strategy-sqpoll")]
pub(crate) fn build_ring(
    entries: u32,
    sqpoll_config: SqpollConfig,
    sqpoll_cpu: Option<usize>,
) -> Result<IoUring> {
    let mut builder = IoUring::builder();
    builder.setup_clamp();
    builder.setup_sqpoll(sqpoll_config.idle_ms);
    builder.setup_single_issuer();
    if let Some(cpu) = sqpoll_cpu {
        builder.setup_sqpoll_cpu(cpu as u32);
    }

    match builder.build(entries) {
        Ok(ring) => {
            if ring.params().is_feature_sqpoll_nonfixed() {
                Ok(ring)
            } else {
                Err(NifError::from_errno(libc::EOPNOTSUPP))
            }
        }
        Err(err) => Err(NifError::from_errno(
            err.raw_os_error().unwrap_or(libc::EIO),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_reactor::SqpollMode;

    #[test]
    fn test_build_ring_ring_size_clamping() {
        // io_uring accepts a maximum of 2^32-1 entries, but practical limits
        // are much lower. A size of 0 should be rejected by the builder.
        let config = SqpollConfig {
            mode: SqpollMode::Off,
            cpu: crate::runtime_reactor::SqpollCpu::None,
            idle_ms: 0,
        };
        let result = build_ring(0, config, None);
        // The io_uring builder should reject 0 entries; we verify the error
        // path is reached without panicking.
        assert!(result.is_err());
    }

    #[test]
    fn test_sqpoll_unsupported_feature_mapping() {
        // When SQPOLL is requested via SqpollMode::Require but the kernel does
        // not support it, the error should be mapped to EOPNOTSUPP.
        let config = SqpollConfig {
            mode: SqpollMode::Require,
            cpu: crate::runtime_reactor::SqpollCpu::None,
            idle_ms: 500,
        };
        let result = build_ring(32, config, None);
        // On a system where SQPOLL is not available, we expect an error.
        // The specific errno may be ENOSYS, EINVAL, EPERM, or EOPNOTSUPP.
        if let Err(NifError::Errno(errno)) = result {
            assert!(
                matches!(
                    errno,
                    libc::ENOSYS | libc::EINVAL | libc::EPERM | libc::EOPNOTSUPP
                ),
                "expected SQPOLL unsupported errno, got {}",
                errno
            );
        }
        // If the system supports SQPOLL (rare in test envs), the test passes
        // because we got Ok(ring) instead of an error.
    }

    #[test]
    fn test_probe_err_maps_enotsup() {
        // map_probe_err should convert ENOTSUPP to the canonical feature error.
        let io_err = std::io::Error::from_raw_os_error(libc::EOPNOTSUPP);
        let nif_err = map_probe_err(io_err);
        match nif_err {
            NifError::Errno(errno) => {
                assert_eq!(errno, ERRNO_FEAT_NOT_SUPPORTED);
            }
            _ => panic!("expected NifError::Errno"),
        }
    }

    #[test]
    fn test_probe_err_preserves_other_errnos() {
        // map_probe_err should pass through errnos other than ENOSYS/EINVAL/EOPNOTSUPP.
        let io_err = std::io::Error::from_raw_os_error(libc::ENOMEM);
        let nif_err = map_probe_err(io_err);
        match nif_err {
            NifError::Errno(errno) => assert_eq!(errno, libc::ENOMEM),
            _ => panic!("expected NifError::Errno"),
        }
    }

    #[test]
    fn test_feature_error_maps_enotsup() {
        let io_err = std::io::Error::from_raw_os_error(libc::EOPNOTSUPP);
        let nif_err = map_feature_error(io_err);
        match nif_err {
            NifError::Errno(errno) => {
                assert_eq!(errno, ERRNO_FEAT_NOT_SUPPORTED);
            }
            _ => panic!("expected NifError::Errno"),
        }
    }

    #[test]
    fn test_feature_error_preserves_einval() {
        // map_feature_error treats EINVAL differently from ENOSYS/EOPNOTSUPP.
        let io_err = std::io::Error::from_raw_os_error(libc::EINVAL);
        let nif_err = map_feature_error(io_err);
        match nif_err {
            NifError::Errno(errno) => assert_eq!(errno, libc::EINVAL),
            _ => panic!("expected NifError::Errno"),
        }
    }
}
