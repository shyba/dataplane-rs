#[cfg(all(feature = "ingress-kanal", feature = "ingress-local"))]
compile_error!("enable only one ingress backend feature");

#[cfg(not(any(feature = "ingress-kanal", feature = "ingress-local")))]
compile_error!("enable one ingress backend feature");

#[cfg(feature = "ingress-kanal")]
pub(crate) type IngressSender<T> = kanal::Sender<Vec<T>>;
#[cfg(feature = "ingress-kanal")]
pub(crate) type IngressReceiver<T> = kanal::Receiver<Vec<T>>;

pub(crate) const INGRESS_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IngressSendError {
    Closed,
    Overloaded,
}

#[cfg(feature = "ingress-local")]
use std::collections::VecDeque;
#[cfg(feature = "ingress-local")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "ingress-local")]
#[derive(Clone)]
pub(crate) struct IngressSender<T> {
    inner: Arc<Mutex<VecDeque<Vec<T>>>>,
}

#[cfg(feature = "ingress-local")]
pub(crate) struct IngressReceiver<T> {
    inner: Arc<Mutex<VecDeque<Vec<T>>>>,
}

#[cfg(feature = "ingress-kanal")]
pub(crate) fn ingress_channel<T>() -> (IngressSender<T>, IngressReceiver<T>) {
    kanal::bounded::<Vec<T>>(INGRESS_CAPACITY)
}

#[cfg(feature = "ingress-local")]
pub(crate) fn ingress_channel<T>() -> (IngressSender<T>, IngressReceiver<T>) {
    let inner = Arc::new(Mutex::new(VecDeque::with_capacity(INGRESS_CAPACITY)));
    (
        IngressSender {
            inner: inner.clone(),
        },
        IngressReceiver { inner },
    )
}

#[cfg(feature = "ingress-kanal")]
pub(crate) fn ingress_send<T>(
    tx: &IngressSender<T>,
    batch: Vec<T>,
) -> Result<(), IngressSendError> {
    match tx.try_send(batch) {
        Ok(true) => Ok(()),
        Ok(false) => Err(IngressSendError::Overloaded),
        Err(_) => Err(IngressSendError::Closed),
    }
}

#[cfg(feature = "ingress-local")]
pub(crate) fn ingress_send<T>(
    tx: &IngressSender<T>,
    batch: Vec<T>,
) -> Result<(), IngressSendError> {
    let mut guard = tx.inner.lock().map_err(|_| IngressSendError::Closed)?;
    if guard.len() >= INGRESS_CAPACITY {
        return Err(IngressSendError::Overloaded);
    }
    guard.push_back(batch);
    Ok(())
}

/// Receives one published ingress batch without flattening its command grouping.
///
/// The hosted shard step depends on this boundary preserving batch boundaries so
/// `drain_commands()` can apply its own budget and deferral policy after a single
/// batch dequeue.
#[cfg(feature = "ingress-kanal")]
pub(crate) fn ingress_try_recv<T>(rx: &IngressReceiver<T>) -> Option<Vec<T>> {
    rx.try_recv().ok().flatten()
}

#[cfg(feature = "ingress-local")]
pub(crate) fn ingress_try_recv<T>(rx: &IngressReceiver<T>) -> Option<Vec<T>> {
    rx.inner.lock().ok()?.pop_front()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingress_try_recv_returns_one_published_batch_intact() {
        let (tx, rx) = ingress_channel::<usize>();
        let batch = vec![11, 22, 33];

        ingress_send(&tx, batch).expect("send batch");

        assert_eq!(ingress_try_recv(&rx), Some(vec![11, 22, 33]));
        assert_eq!(ingress_try_recv(&rx), None);
    }

    #[test]
    fn ingress_send_reports_overload_at_capacity() {
        let (tx, _rx) = ingress_channel::<usize>();

        for i in 0..INGRESS_CAPACITY {
            ingress_send(&tx, vec![i]).expect("send within capacity");
        }

        assert_eq!(
            ingress_send(&tx, vec![INGRESS_CAPACITY]),
            Err(IngressSendError::Overloaded)
        );
    }
}
