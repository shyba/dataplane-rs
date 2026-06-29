use crate::reactor::{Handler, NetSubscription, NetSubscriptionEvent};

pub(super) struct SubscriptionState {
    pub(super) kind: NetSubscription,
    pub(super) handler: Box<dyn Handler<NetSubscriptionEvent>>,
}
