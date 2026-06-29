use std::sync::{Arc, Mutex};

use crossbeam_queue::SegQueue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalIngressFull;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LocalIngressSlot(usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalIngressStaging<T> {
    items: Vec<T>,
}

impl<T> LocalIngressStaging<T> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn publish_into(&mut self, ingress: &LocalIngress<T>) -> usize {
        ingress.publish_staging(self)
    }

    pub fn try_publish_into(
        &mut self,
        ingress: &LocalIngress<T>,
    ) -> Result<usize, LocalIngressFull> {
        ingress.try_publish_staging(self)
    }
}

impl<T> Default for LocalIngressStaging<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LocalIngress<T> {
    slots: Arc<Vec<Mutex<Vec<T>>>>,
    ready: Arc<SegQueue<usize>>,
    free: Arc<SegQueue<usize>>,
}

impl<T> Clone for LocalIngress<T> {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            ready: self.ready.clone(),
            free: self.free.clone(),
        }
    }
}

impl<T> LocalIngress<T> {
    pub fn new(slot_count: usize, slot_capacity: usize) -> Self {
        let slots = Arc::new(
            (0..slot_count)
                .map(|_| Mutex::new(Vec::with_capacity(slot_capacity)))
                .collect(),
        );
        let ready = Arc::new(SegQueue::new());
        let free = Arc::new(SegQueue::new());
        for idx in 0..slot_count {
            free.push(idx);
        }
        Self { slots, ready, free }
    }

    pub fn publish_from_staging(&self, staging: &mut Vec<T>) -> usize {
        if staging.is_empty() {
            return 0;
        }
        let idx = loop {
            if let Some(idx) = self.try_acquire_free_slot() {
                break idx;
            }
            std::thread::yield_now();
        };
        self.publish_vec_into_slot(idx, staging)
    }

    pub fn publish_staging(&self, staging: &mut LocalIngressStaging<T>) -> usize {
        if staging.is_empty() {
            return 0;
        }
        let idx = loop {
            if let Some(idx) = self.try_acquire_free_slot() {
                break idx;
            }
            std::thread::yield_now();
        };
        self.publish_staging_into_slot(idx, staging)
    }

    pub fn publish_one(&self, item: T) -> usize {
        let idx = loop {
            if let Some(idx) = self.try_acquire_free_slot() {
                break idx;
            }
            std::thread::yield_now();
        };
        let mut slot = self.slots[idx]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.push(item);
        drop(slot);
        self.ready.push(idx);
        1
    }

    pub fn try_publish_from_staging(
        &self,
        staging: &mut Vec<T>,
    ) -> Result<usize, LocalIngressFull> {
        if staging.is_empty() {
            return Ok(0);
        }
        let Some(idx) = self.try_acquire_free_slot() else {
            return Err(LocalIngressFull);
        };
        Ok(self.publish_vec_into_slot(idx, staging))
    }

    pub fn try_publish_staging(
        &self,
        staging: &mut LocalIngressStaging<T>,
    ) -> Result<usize, LocalIngressFull> {
        if staging.is_empty() {
            return Ok(0);
        }
        let Some(idx) = self.try_acquire_free_slot() else {
            return Err(LocalIngressFull);
        };
        Ok(self.publish_staging_into_slot(idx, staging))
    }

    pub fn try_publish_one(&self, item: T) -> Result<usize, LocalIngressFull> {
        let Some(idx) = self.try_acquire_free_slot() else {
            return Err(LocalIngressFull);
        };
        let mut slot = self.slots[idx]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slot.push(item);
        drop(slot);
        self.ready.push(idx);
        Ok(1)
    }

    pub fn pop_ready(&self) -> Option<LocalIngressSlot> {
        self.ready.pop().map(LocalIngressSlot)
    }

    pub fn consume_ready<R>(&self, f: impl FnOnce(&mut Vec<T>) -> R) -> Option<R> {
        let slot = self.pop_ready()?;
        let result = self.with_slot(slot, f);
        self.release(slot);
        Some(result)
    }

    pub fn with_slot<R>(&self, slot: LocalIngressSlot, f: impl FnOnce(&mut Vec<T>) -> R) -> R {
        let mut contents = self.slots[slot.0]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut *contents)
    }

    pub fn release(&self, slot: LocalIngressSlot) {
        let idx = slot.0;
        let mut contents = self.slots[idx]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        contents.clear();
        drop(contents);
        self.free.push(idx);
    }

    fn try_acquire_free_slot(&self) -> Option<usize> {
        self.free.pop()
    }

    fn publish_vec_into_slot(&self, idx: usize, staging: &mut Vec<T>) -> usize {
        let len = staging.len();
        let mut slot = self.slots[idx]
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::swap(&mut *slot, staging);
        drop(slot);
        self.ready.push(idx);
        len
    }

    fn publish_staging_into_slot(&self, idx: usize, staging: &mut LocalIngressStaging<T>) -> usize {
        self.publish_vec_into_slot(idx, &mut staging.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_publish_returns_full_when_no_free_slot_exists() {
        let ingress = LocalIngress::<u32>::new(1, 4);

        assert_eq!(ingress.try_publish_one(1), Ok(1));
        assert_eq!(ingress.try_publish_one(2), Err(LocalIngressFull));
    }

    #[test]
    fn try_publish_from_staging_swaps_payload_and_preserves_release_flow() {
        let ingress = LocalIngress::<u32>::new(1, 4);
        let mut staging = vec![1, 2, 3];

        assert_eq!(ingress.try_publish_from_staging(&mut staging), Ok(3));
        assert!(staging.is_empty());

        let idx = ingress.pop_ready().expect("slot should be ready");
        let values = ingress.with_slot(idx, |slot| slot.clone());
        assert_eq!(values, vec![1, 2, 3]);

        ingress.release(idx);

        assert_eq!(ingress.try_publish_one(9), Ok(1));
        let idx = ingress.pop_ready().expect("slot should be ready again");
        let values = ingress.with_slot(idx, |slot| slot.clone());
        assert_eq!(values, vec![9]);
    }

    #[test]
    fn ready_slot_handle_roundtrips_through_release_and_reuse() {
        let ingress = LocalIngress::<u32>::new(1, 2);

        assert_eq!(ingress.try_publish_one(7), Ok(1));
        let slot = ingress.pop_ready().expect("slot should be ready");
        assert_eq!(ingress.with_slot(slot, |items| items.clone()), vec![7]);

        ingress.release(slot);

        assert_eq!(ingress.try_publish_from_staging(&mut vec![8, 9]), Ok(2));
        let reused = ingress.pop_ready().expect("slot should be ready again");
        assert_eq!(reused, slot);
        assert_eq!(ingress.with_slot(reused, |items| items.clone()), vec![8, 9]);
    }

    #[test]
    fn explicit_staging_wrapper_preserves_publish_and_reuse_flow() {
        let ingress = LocalIngress::<u32>::new(1, 4);
        let mut staging = LocalIngressStaging::with_capacity(4);
        staging.push(10);
        staging.push(11);
        staging.push(12);

        assert_eq!(ingress.try_publish_staging(&mut staging), Ok(3));
        assert!(staging.is_empty());

        let slot = ingress.pop_ready().expect("slot should be ready");
        assert_eq!(
            ingress.with_slot(slot, |items| items.clone()),
            vec![10, 11, 12]
        );
        ingress.release(slot);

        staging.push(13);
        assert_eq!(ingress.publish_staging(&mut staging), 1);
        assert!(staging.is_empty());

        let slot = ingress.pop_ready().expect("slot should be ready again");
        assert_eq!(ingress.with_slot(slot, |items| items.clone()), vec![13]);
    }

    #[test]
    fn consume_ready_returns_payload_and_releases_slot_for_reuse() {
        let ingress = LocalIngress::<u32>::new(1, 4);

        assert_eq!(ingress.try_publish_from_staging(&mut vec![21, 22]), Ok(2));
        let values = ingress
            .consume_ready(|items| items.clone())
            .expect("slot should be ready");
        assert_eq!(values, vec![21, 22]);

        assert_eq!(ingress.try_publish_one(23), Ok(1));
        let values = ingress
            .consume_ready(|items| items.clone())
            .expect("slot should be ready again");
        assert_eq!(values, vec![23]);
    }

    #[test]
    fn staging_wrapper_can_publish_itself() {
        let ingress = LocalIngress::<u32>::new(1, 4);
        let mut staging = LocalIngressStaging::with_capacity(4);
        staging.push(31);
        staging.push(32);

        assert_eq!(staging.try_publish_into(&ingress), Ok(2));
        assert!(staging.is_empty());
        assert_eq!(
            ingress
                .consume_ready(|items| items.clone())
                .expect("slot should be ready"),
            vec![31, 32]
        );

        staging.push(33);
        assert_eq!(staging.publish_into(&ingress), 1);
        assert!(staging.is_empty());
        assert_eq!(
            ingress
                .consume_ready(|items| items.clone())
                .expect("slot should be ready again"),
            vec![33]
        );
    }
}
