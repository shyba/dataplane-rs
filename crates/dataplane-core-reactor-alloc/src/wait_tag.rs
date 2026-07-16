#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitTag(u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WaitDomain {
    Remote = 0,
    Local = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LocalWaitKind {
    Runnable = 0,
    Yield = 1,
    Join = 2,
    Timer = 3,
    Io = 4,
    Select = 5,
    Channel = 6,
    Cancelled = 7,
    Extended = 31,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitRef {
    Remote(u16),
    Local { kind: LocalWaitKind, payload: u16 },
}

/// Raw wait-tag bits that do not decode to a known kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitDecodeError {
    /// The 5-bit kind field holds a value with no `LocalWaitKind` mapping.
    BadLocalKind,
}

impl TryFrom<u8> for WaitDomain {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Remote),
            1 => Ok(Self::Local),
            _ => Err(()),
        }
    }
}

impl TryFrom<u8> for LocalWaitKind {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Runnable),
            1 => Ok(Self::Yield),
            2 => Ok(Self::Join),
            3 => Ok(Self::Timer),
            4 => Ok(Self::Io),
            5 => Ok(Self::Select),
            6 => Ok(Self::Channel),
            7 => Ok(Self::Cancelled),
            31 => Ok(Self::Extended),
            _ => Err(()),
        }
    }
}

impl WaitTag {
    pub const DOMAIN_MASK: u16 = 0b1;
    pub const REMOTE_INDEX_SHIFT: u16 = 1;
    pub const REMOTE_INDEX_MASK: u16 = 0x7fff;
    pub const LOCAL_KIND_SHIFT: u16 = 1;
    pub const LOCAL_KIND_MASK: u16 = 0b1_1111;
    pub const LOCAL_PAYLOAD_SHIFT: u16 = 6;
    pub const LOCAL_PAYLOAD_MASK: u16 = 0b11_1111_1111;

    #[inline(always)]
    pub const fn new_remote(index: u16) -> Self {
        debug_assert!(index <= Self::REMOTE_INDEX_MASK);
        // Mask defensively (matching the no_std twin) so out-of-range input
        // cannot corrupt adjacent bitfields in release builds.
        Self((index & Self::REMOTE_INDEX_MASK) << Self::REMOTE_INDEX_SHIFT)
    }

    #[inline(always)]
    pub const fn new_local(kind: LocalWaitKind, payload: u16) -> Self {
        debug_assert!(payload <= Self::LOCAL_PAYLOAD_MASK);
        Self(
            WaitDomain::Local as u16
                | ((kind as u16) << Self::LOCAL_KIND_SHIFT)
                | ((payload & Self::LOCAL_PAYLOAD_MASK) << Self::LOCAL_PAYLOAD_SHIFT),
        )
    }

    #[inline(always)]
    pub const fn raw(self) -> u16 {
        self.0
    }

    #[inline(always)]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    #[inline(always)]
    pub fn domain(self) -> WaitDomain {
        let raw = (self.0 & Self::DOMAIN_MASK) as u8;
        WaitDomain::try_from(raw).unwrap()
    }

    #[inline(always)]
    pub fn remote_index(self) -> u16 {
        debug_assert_eq!(self.domain(), WaitDomain::Remote);
        (self.0 >> Self::REMOTE_INDEX_SHIFT) & Self::REMOTE_INDEX_MASK
    }

    /// Decodes the local wait kind. `from_raw` admits kind bits with no
    /// `LocalWaitKind` mapping (8-30), so this is fallible — matching the
    /// no_std twin instead of panicking.
    #[inline(always)]
    pub fn local_kind(self) -> Result<LocalWaitKind, WaitDecodeError> {
        debug_assert_eq!(self.domain(), WaitDomain::Local);
        let raw = ((self.0 >> Self::LOCAL_KIND_SHIFT) & Self::LOCAL_KIND_MASK) as u8;
        LocalWaitKind::try_from(raw).map_err(|_| WaitDecodeError::BadLocalKind)
    }

    #[inline(always)]
    pub fn local_payload(self) -> u16 {
        debug_assert_eq!(self.domain(), WaitDomain::Local);
        (self.0 >> Self::LOCAL_PAYLOAD_SHIFT) & Self::LOCAL_PAYLOAD_MASK
    }

    #[inline(always)]
    pub fn remote_ref(self) -> Option<u16> {
        match self.domain() {
            WaitDomain::Remote => Some(self.remote_index()),
            WaitDomain::Local => None,
        }
    }

    #[inline(always)]
    pub fn local_ref(self) -> Option<(LocalWaitKind, u16)> {
        match self.domain() {
            WaitDomain::Remote => None,
            WaitDomain::Local => self
                .local_kind()
                .ok()
                .map(|kind| (kind, self.local_payload())),
        }
    }

    #[inline(always)]
    pub fn decode(self) -> Result<WaitRef, WaitDecodeError> {
        if let Some(index) = self.remote_ref() {
            return Ok(WaitRef::Remote(index));
        }
        let kind = self.local_kind()?;
        Ok(WaitRef::Local {
            kind,
            payload: self.local_payload(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalWaitKind, WaitDomain, WaitRef, WaitTag};
    use proptest::prelude::*;

    #[test]
    fn wait_tag_is_u16() {
        assert_eq!(core::mem::size_of::<WaitTag>(), 2);
    }

    #[test]
    fn remote_round_trips() {
        let tag = WaitTag::new_remote(32123);
        assert_eq!(tag.domain(), WaitDomain::Remote);
        assert_eq!(tag.remote_index(), 32123);
        assert_eq!(tag.decode(), Ok(WaitRef::Remote(32123)));
    }

    #[test]
    fn local_round_trips() {
        let tag = WaitTag::new_local(LocalWaitKind::Io, 777);
        assert_eq!(tag.domain(), WaitDomain::Local);
        assert_eq!(tag.local_kind(), Ok(LocalWaitKind::Io));
        assert_eq!(tag.local_payload(), 777);
        assert_eq!(tag.local_ref(), Some((LocalWaitKind::Io, 777)));
        assert_eq!(tag.remote_ref(), None);
        assert_eq!(
            tag.decode(),
            Ok(WaitRef::Local {
                kind: LocalWaitKind::Io,
                payload: 777,
            })
        );
    }

    #[test]
    fn raw_round_trips() {
        let raw = WaitTag::new_local(LocalWaitKind::Extended, 1023).raw();
        let tag = WaitTag::from_raw(raw);
        assert_eq!(tag.local_ref(), Some((LocalWaitKind::Extended, 1023)));
        assert_eq!(
            tag.decode(),
            Ok(WaitRef::Local {
                kind: LocalWaitKind::Extended,
                payload: 1023,
            })
        );
    }

    proptest! {
        #[test]
        fn remote_round_trips_for_any_valid_index(index in 0u16..=WaitTag::REMOTE_INDEX_MASK) {
            let tag = WaitTag::new_remote(index);
            prop_assert_eq!(tag.domain(), WaitDomain::Remote);
            prop_assert_eq!(tag.remote_index(), index);
            prop_assert_eq!(tag.remote_ref(), Some(index));
            prop_assert_eq!(tag.local_ref(), None);
            prop_assert_eq!(WaitTag::from_raw(tag.raw()).decode(), Ok(WaitRef::Remote(index)));
        }

        #[test]
        fn local_round_trips_for_any_valid_payload(
            kind in prop_oneof![
                Just(LocalWaitKind::Runnable),
                Just(LocalWaitKind::Yield),
                Just(LocalWaitKind::Join),
                Just(LocalWaitKind::Timer),
                Just(LocalWaitKind::Io),
                Just(LocalWaitKind::Select),
                Just(LocalWaitKind::Channel),
                Just(LocalWaitKind::Cancelled),
                Just(LocalWaitKind::Extended),
            ],
            payload in 0u16..=WaitTag::LOCAL_PAYLOAD_MASK
        ) {
            let tag = WaitTag::new_local(kind, payload);
            prop_assert_eq!(tag.domain(), WaitDomain::Local);
            prop_assert_eq!(tag.local_kind(), Ok(kind));
            prop_assert_eq!(tag.local_payload(), payload);
            prop_assert_eq!(tag.local_ref(), Some((kind, payload)));
            prop_assert_eq!(tag.remote_ref(), None);
            prop_assert_eq!(
                WaitTag::from_raw(tag.raw()).decode(),
                Ok(WaitRef::Local { kind, payload })
            );
        }
    }
}
