use core::cmp;

pub const FLASH_PAGE_SIZE: usize = 256;
pub const FLASH_SECTOR_SIZE: usize = 4096;
pub const ALLOCATOR_BYTES: usize = FLASH_SECTOR_SIZE;
pub const RAW_MEASUREMENT_BYTES: usize = 9;
pub const RECORDS_PER_PAGE: usize = 27;
pub const RECORD_PAYLOAD_BYTES: usize = RAW_MEASUREMENT_BYTES * RECORDS_PER_PAGE;
pub const PAGE_MAGIC: u8 = 0xD1;
pub const PAGES_PER_SECTOR: usize = FLASH_SECTOR_SIZE / FLASH_PAGE_SIZE;

const COUNTER_OFFSET: usize = 1;
const PAYLOAD_OFFSET: usize = COUNTER_OFFSET + core::mem::size_of::<u64>();
const CRC_OFFSET: usize = PAYLOAD_OFFSET + RECORD_PAYLOAD_BYTES;

const _: () = assert!(CRC_OFFSET + core::mem::size_of::<u32>() == FLASH_PAGE_SIZE);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    OutOfBounds,
    ProgramConflict,
    Io,
}

pub trait Flash {
    fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), FlashError>;
    fn program(&mut self, offset: usize, data: &[u8]) -> Result<(), FlashError>;
    fn erase_sector(&mut self, offset: usize) -> Result<(), FlashError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub allocator_offset: usize,
    pub data_offset: usize,
    pub page_count: usize,
}

impl Geometry {
    pub const fn new(allocator_offset: usize, data_offset: usize, page_count: usize) -> Self {
        Self {
            allocator_offset,
            data_offset,
            page_count,
        }
    }

    pub const fn page_offset(self, slot: usize) -> usize {
        self.data_offset + slot * FLASH_PAGE_SIZE
    }

    pub const fn data_sector_offset(self, slot: usize) -> usize {
        self.page_offset(slot - (slot % PAGES_PER_SECTOR))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedPage {
    pub slot: usize,
    pub counter: u64,
    pub payload: [u8; RECORD_PAYLOAD_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recovery {
    pub newest_slot: Option<usize>,
    pub newest_counter: u64,
    pub valid_pages: usize,
    pub next_slot: usize,
    pub next_counter: u64,
}

impl Recovery {
    const fn empty() -> Self {
        Self {
            newest_slot: None,
            newest_counter: 0,
            valid_pages: 0,
            next_slot: 0,
            next_counter: 0,
        }
    }
}

pub trait PageSink {
    fn page(&mut self, page: &DecodedPage);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollingLog {
    geometry: Geometry,
    recovery: Recovery,
}

impl RollingLog {
    pub fn recover<F: Flash>(flash: &F, geometry: Geometry) -> Result<Self, FlashError> {
        let mut recovery = Recovery::empty();

        for slot in 0..geometry.page_count {
            if allocator_bit_is_free(flash, geometry, slot)? {
                continue;
            }

            if let Some(page) = read_valid_page(flash, geometry, slot)? {
                recovery.valid_pages += 1;
                if recovery.newest_slot.is_none()
                    || counter_after(page.counter, recovery.newest_counter)
                {
                    recovery.newest_slot = Some(slot);
                    recovery.newest_counter = page.counter;
                }
            }
        }

        if let Some(newest_slot) = recovery.newest_slot {
            recovery.next_slot = (newest_slot + 1) % geometry.page_count;
            recovery.next_counter = recovery.newest_counter.wrapping_add(1);
        }

        Ok(Self { geometry, recovery })
    }

    pub fn recover_or_initialize<F: Flash>(
        flash: &mut F,
        geometry: Geometry,
    ) -> Result<Self, FlashError> {
        let recovered = Self::recover(flash, geometry)?;
        if recovered.recovery.newest_slot.is_some() {
            return Ok(recovered);
        }

        flash.erase_sector(geometry.allocator_offset)?;
        Self::recover(flash, geometry)
    }

    pub const fn recovery(&self) -> Recovery {
        self.recovery
    }

    pub fn append_page<F: Flash>(
        &mut self,
        flash: &mut F,
        payload: &[u8; RECORD_PAYLOAD_BYTES],
    ) -> Result<(), FlashError> {
        let slot = self.recovery.next_slot;

        if slot % PAGES_PER_SECTOR == 0 {
            flash.erase_sector(self.geometry.data_sector_offset(slot))?;
        }

        let mut page = [0xFF; FLASH_PAGE_SIZE];
        encode_page(self.recovery.next_counter, payload, &mut page);
        flash.program(self.geometry.page_offset(slot), &page)?;

        let decoded = read_valid_page(flash, self.geometry, slot)?;
        if decoded
            .as_ref()
            .map(|page| page.counter != self.recovery.next_counter || page.payload != *payload)
            .unwrap_or(true)
        {
            return Err(FlashError::Io);
        }

        commit_allocator_bit(flash, self.geometry, slot)?;

        self.recovery.newest_slot = Some(slot);
        self.recovery.newest_counter = self.recovery.next_counter;
        self.recovery.next_counter = self.recovery.next_counter.wrapping_add(1);
        self.recovery.next_slot = (slot + 1) % self.geometry.page_count;
        self.recovery.valid_pages =
            cmp::min(self.recovery.valid_pages + 1, self.geometry.page_count);
        Ok(())
    }

    pub fn read_tail<F: Flash, S: PageSink>(
        &self,
        flash: &F,
        amount: usize,
        sink: &mut S,
    ) -> Result<usize, FlashError> {
        let Some(newest_slot) = self.recovery.newest_slot else {
            return Ok(0);
        };

        let count = cmp::min(amount, self.recovery.valid_pages);
        let mut emitted = 0usize;

        for age in (0..count).rev() {
            let slot = (newest_slot + self.geometry.page_count - (age % self.geometry.page_count))
                % self.geometry.page_count;
            if allocator_bit_is_free(flash, self.geometry, slot)? {
                continue;
            }

            if let Some(page) = read_valid_page(flash, self.geometry, slot)? {
                sink.page(&page);
                emitted += 1;
            }
        }

        Ok(emitted)
    }

    pub fn read_age<F: Flash>(
        &self,
        flash: &F,
        age_from_newest: usize,
    ) -> Result<Option<DecodedPage>, FlashError> {
        let Some(newest_slot) = self.recovery.newest_slot else {
            return Ok(None);
        };
        if age_from_newest >= self.recovery.valid_pages {
            return Ok(None);
        }

        let slot = (newest_slot + self.geometry.page_count
            - (age_from_newest % self.geometry.page_count))
            % self.geometry.page_count;
        if allocator_bit_is_free(flash, self.geometry, slot)? {
            return Ok(None);
        }

        read_valid_page(flash, self.geometry, slot)
    }
}

pub fn encode_page(counter: u64, payload: &[u8; RECORD_PAYLOAD_BYTES], out: &mut [u8; 256]) {
    out.fill(0xFF);
    out[0] = PAGE_MAGIC;
    out[COUNTER_OFFSET..PAYLOAD_OFFSET].copy_from_slice(&counter.to_le_bytes());
    out[PAYLOAD_OFFSET..CRC_OFFSET].copy_from_slice(payload);
    let crc = crc32(&out[..CRC_OFFSET]);
    out[CRC_OFFSET..].copy_from_slice(&crc.to_le_bytes());
}

pub fn decode_page(slot: usize, page: &[u8; FLASH_PAGE_SIZE]) -> Option<DecodedPage> {
    if page[0] != PAGE_MAGIC {
        return None;
    }

    let expected = u32::from_le_bytes(page[CRC_OFFSET..].try_into().ok()?);
    if crc32(&page[..CRC_OFFSET]) != expected {
        return None;
    }

    let counter = u64::from_le_bytes(page[COUNTER_OFFSET..PAYLOAD_OFFSET].try_into().ok()?);
    let mut payload = [0u8; RECORD_PAYLOAD_BYTES];
    payload.copy_from_slice(&page[PAYLOAD_OFFSET..CRC_OFFSET]);
    Some(DecodedPage {
        slot,
        counter,
        payload,
    })
}

pub const fn counter_after(a: u64, b: u64) -> bool {
    let distance = a.wrapping_sub(b);
    distance != 0 && distance < (1u64 << 63)
}

#[cfg(test)]
fn find_counter<F: Flash>(
    flash: &F,
    geometry: Geometry,
    counter: u64,
) -> Result<Option<DecodedPage>, FlashError> {
    for slot in 0..geometry.page_count {
        if allocator_bit_is_free(flash, geometry, slot)? {
            continue;
        }

        if let Some(page) = read_valid_page(flash, geometry, slot)? {
            if page.counter == counter {
                return Ok(Some(page));
            }
        }
    }

    Ok(None)
}

fn read_valid_page<F: Flash>(
    flash: &F,
    geometry: Geometry,
    slot: usize,
) -> Result<Option<DecodedPage>, FlashError> {
    let mut page = [0u8; FLASH_PAGE_SIZE];
    flash.read(geometry.page_offset(slot), &mut page)?;
    Ok(decode_page(slot, &page))
}

fn allocator_bit_is_free<F: Flash>(
    flash: &F,
    geometry: Geometry,
    slot: usize,
) -> Result<bool, FlashError> {
    let mut byte = [0u8; 1];
    flash.read(geometry.allocator_offset + slot / 8, &mut byte)?;
    Ok((byte[0] & (1u8 << (slot % 8))) != 0)
}

fn commit_allocator_bit<F: Flash>(
    flash: &mut F,
    geometry: Geometry,
    slot: usize,
) -> Result<(), FlashError> {
    let mut byte = [0u8; 1];
    let offset = geometry.allocator_offset + slot / 8;
    flash.read(offset, &mut byte)?;
    byte[0] &= !(1u8 << (slot % 8));
    flash.program(offset, &byte)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::collections::VecDeque;
    use std::vec::Vec;

    const TEST_PAGES: usize = 32;
    const TEST_FLASH_BYTES: usize = ALLOCATOR_BYTES + TEST_PAGES * FLASH_PAGE_SIZE;
    const TEST_GEOMETRY: Geometry = Geometry::new(0, ALLOCATOR_BYTES, TEST_PAGES);

    #[derive(Clone)]
    struct ModelFlash {
        bytes: [u8; TEST_FLASH_BYTES],
        fault: Option<Fault>,
    }

    #[derive(Clone, Copy)]
    enum Fault {
        ProgramPrefix(usize),
        ErasePrefix(usize),
    }

    impl ModelFlash {
        fn new() -> Self {
            Self {
                bytes: [0xFF; TEST_FLASH_BYTES],
                fault: None,
            }
        }

        fn with_fault(mut self, fault: Fault) -> Self {
            self.fault = Some(fault);
            self
        }
    }

    impl Flash for ModelFlash {
        fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), FlashError> {
            let Some(end) = offset.checked_add(out.len()) else {
                return Err(FlashError::OutOfBounds);
            };
            if end > self.bytes.len() {
                return Err(FlashError::OutOfBounds);
            }
            out.copy_from_slice(&self.bytes[offset..end]);
            Ok(())
        }

        fn program(&mut self, offset: usize, data: &[u8]) -> Result<(), FlashError> {
            let Some(end) = offset.checked_add(data.len()) else {
                return Err(FlashError::OutOfBounds);
            };
            if end > self.bytes.len() {
                return Err(FlashError::OutOfBounds);
            }

            let limit = match self.fault {
                Some(Fault::ProgramPrefix(prefix)) => {
                    self.fault = None;
                    cmp::min(prefix, data.len())
                }
                _ => data.len(),
            };

            for (index, byte) in data.iter().copied().take(limit).enumerate() {
                let current = self.bytes[offset + index];
                if (current & byte) != byte {
                    return Err(FlashError::ProgramConflict);
                }
                self.bytes[offset + index] = current & byte;
            }

            if limit != data.len() {
                return Err(FlashError::Io);
            }

            Ok(())
        }

        fn erase_sector(&mut self, offset: usize) -> Result<(), FlashError> {
            if offset % FLASH_SECTOR_SIZE != 0 || offset >= self.bytes.len() {
                return Err(FlashError::OutOfBounds);
            }

            let limit = match self.fault {
                Some(Fault::ErasePrefix(prefix)) => {
                    self.fault = None;
                    cmp::min(prefix, FLASH_SECTOR_SIZE)
                }
                _ => FLASH_SECTOR_SIZE,
            };
            let end = cmp::min(offset + limit, self.bytes.len());
            self.bytes[offset..end].fill(0xFF);

            if limit != FLASH_SECTOR_SIZE {
                return Err(FlashError::Io);
            }

            Ok(())
        }
    }

    #[derive(Default)]
    struct CollectSink {
        pages: Vec<DecodedPage>,
    }

    impl PageSink for CollectSink {
        fn page(&mut self, page: &DecodedPage) {
            self.pages.push(*page);
        }
    }

    fn payload(counter: u64) -> [u8; RECORD_PAYLOAD_BYTES] {
        let mut out = [0u8; RECORD_PAYLOAD_BYTES];
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = counter.wrapping_add(index as u64) as u8;
        }
        out
    }

    fn prune_erased_slots(live: &mut VecDeque<(usize, u64)>, slot: usize, erased_bytes: usize) {
        if slot % PAGES_PER_SECTOR != 0 {
            return;
        }

        let erased_pages = cmp::min(
            PAGES_PER_SECTOR,
            erased_bytes.saturating_add(FLASH_PAGE_SIZE - 1) / FLASH_PAGE_SIZE,
        );
        let sector_start = slot - (slot % PAGES_PER_SECTOR);
        live.retain(|(page_slot, _)| {
            *page_slot < sector_start || *page_slot >= sector_start + erased_pages
        });
    }

    fn record_successful_append(live: &mut VecDeque<(usize, u64)>, slot: usize, counter: u64) {
        prune_erased_slots(live, slot, FLASH_SECTOR_SIZE);
        live.retain(|(page_slot, _)| *page_slot != slot);
        live.push_back((slot, counter));
        while live.len() > TEST_PAGES {
            live.pop_front();
        }
    }

    fn valid_pages(flash: &ModelFlash) -> Vec<DecodedPage> {
        let log = RollingLog::recover(flash, TEST_GEOMETRY).unwrap();
        let Some(_) = log.recovery().newest_slot else {
            return Vec::new();
        };

        let mut pages = Vec::new();
        for age in (0..log.recovery().valid_pages).rev() {
            let wanted = log.recovery().newest_counter.wrapping_sub(age as u64);
            if let Some(page) = find_counter(flash, TEST_GEOMETRY, wanted).unwrap() {
                pages.push(page);
            }
        }
        pages
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]
        #[test]
        fn rolling_log_recovers_committed_suffix(ops in proptest::collection::vec(0u8..=7, 1..96)) {
            let mut flash = ModelFlash::new();
            let mut log = RollingLog::recover(&flash, TEST_GEOMETRY).unwrap();
            let mut committed = VecDeque::<(usize, u64)>::new();

            for op in ops {
                match op {
                    0..=3 => {
                        let slot = log.recovery().next_slot;
                        let counter = log.recovery().next_counter;
                        if log.append_page(&mut flash, &payload(counter)).is_ok() {
                            record_successful_append(&mut committed, slot, counter);
                        }
                    }
                    4 => {
                        log = RollingLog::recover(&flash, TEST_GEOMETRY).unwrap();
                    }
                    5 => {
                        let mut faulted = flash.clone().with_fault(Fault::ProgramPrefix((op as usize) + 3));
                        let mut faulted_log = log;
                        let slot = faulted_log.recovery().next_slot;
                        let counter = faulted_log.recovery().next_counter;
                        if faulted_log.append_page(&mut faulted, &payload(counter)).is_ok() {
                            record_successful_append(&mut committed, slot, counter);
                        }
                        flash = faulted;
                        log = RollingLog::recover(&flash, TEST_GEOMETRY).unwrap();
                    }
                    6 => {
                        let mut faulted = flash.clone().with_fault(Fault::ErasePrefix(FLASH_PAGE_SIZE));
                        let mut faulted_log = log;
                        let slot = faulted_log.recovery().next_slot;
                        let counter = faulted_log.recovery().next_counter;
                        if faulted_log.append_page(&mut faulted, &payload(counter)).is_ok() {
                            record_successful_append(&mut committed, slot, counter);
                        } else {
                            prune_erased_slots(&mut committed, slot, FLASH_PAGE_SIZE);
                        }
                        flash = faulted;
                        log = RollingLog::recover(&flash, TEST_GEOMETRY).unwrap();
                    }
                    _ => {
                        let amount = (op as usize % TEST_PAGES) + 1;
                        let mut sink = CollectSink::default();
                        let emitted = log.read_tail(&flash, amount, &mut sink).unwrap();
                        prop_assert_eq!(emitted, sink.pages.len());
                        prop_assert!(sink.pages.len() <= amount);
                        for pair in sink.pages.windows(2) {
                            prop_assert!(counter_after(pair[1].counter, pair[0].counter));
                        }
                    }
                }

                let recovered = valid_pages(&flash);
                let recovered_counters: Vec<u64> = recovered.iter().map(|page| page.counter).collect();
                let committed_counters: Vec<u64> = committed.iter().map(|(_, counter)| *counter).collect();
                prop_assert!(
                    committed_counters.ends_with(&recovered_counters),
                    "recovered={recovered_counters:?} committed={committed_counters:?}"
                );
                for page in &recovered {
                    prop_assert_eq!(page.payload, payload(page.counter));
                }
                for pair in recovered.windows(2) {
                    prop_assert!(counter_after(pair[1].counter, pair[0].counter));
                }
                prop_assert!(recovered.len() <= TEST_PAGES);

                let log = RollingLog::recover(&flash, TEST_GEOMETRY).unwrap();
                if let Some(newest) = recovered.last() {
                    prop_assert_eq!(log.recovery().newest_counter, newest.counter);
                    prop_assert_eq!(log.recovery().next_counter, newest.counter.wrapping_add(1));
                } else {
                    prop_assert_eq!(log.recovery().next_counter, 0);
                }

                for amount in 0..=cmp::min(TEST_PAGES, 5) {
                    let mut sink = CollectSink::default();
                    let emitted = log.read_tail(&flash, amount, &mut sink).unwrap();
                    let expected_start = recovered.len().saturating_sub(amount);
                    let expected = &recovered[expected_start..];
                    prop_assert_eq!(emitted, expected.len());
                    prop_assert_eq!(sink.pages, expected);
                }
            }
        }
    }

    #[test]
    fn page_codec_rejects_corruption() {
        let mut page = [0u8; FLASH_PAGE_SIZE];
        encode_page(42, &payload(42), &mut page);
        assert!(decode_page(0, &page).is_some());
        page[PAYLOAD_OFFSET + 3] ^= 0x01;
        assert!(decode_page(0, &page).is_none());
    }

    #[test]
    fn wrap_erases_one_sector_of_old_pages() {
        let mut flash = ModelFlash::new();
        let mut log = RollingLog::recover(&flash, TEST_GEOMETRY).unwrap();

        for _ in 0..=TEST_PAGES {
            let counter = log.recovery().next_counter;
            log.append_page(&mut flash, &payload(counter)).unwrap();
        }

        let counters: Vec<u64> = valid_pages(&flash)
            .iter()
            .map(|page| page.counter)
            .collect();
        assert_eq!(
            counters,
            (PAGES_PER_SECTOR as u64..=TEST_PAGES as u64).collect::<Vec<_>>()
        );
    }
}
