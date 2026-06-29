use core::ptr::addr_of_mut;

use dataplane_microkernel_core::{
    CapabilityId, EndpointId, Mailbox, MailboxError, Message, MessageBody, RequestId, TaskId,
};

use crate::mmu;

pub(crate) const PRODUCER_TASK: TaskId = TaskId::new(1);
pub(crate) const NETWORK_TASK: TaskId = TaskId::new(3);
pub(crate) const CONTROL_TASK: TaskId = TaskId::new(4);
pub(crate) const PRODUCER_ENDPOINT: EndpointId = EndpointId::new(1);
pub(crate) const CONSUMER_ENDPOINT: EndpointId = EndpointId::new(2);
pub(crate) const CONTROL_ENDPOINT: EndpointId = EndpointId::new(3);
pub(crate) const NETWORK_ENDPOINT: EndpointId = EndpointId::new(4);
pub(crate) const BENCH_CAPABILITY: CapabilityId = CapabilityId::new(1);
pub(crate) const SERVICE_CAPABILITY: CapabilityId = CapabilityId::new(2);
pub(crate) const SERVICE_REQUEST: RequestId = RequestId::new(1);
pub(crate) const NETWORK_STATUS_OK: u32 = 0x4e45_544f;
pub(crate) const CONTROL_ACK_OK: u32 = 0x4143_4b4f;
pub(crate) const BENCH_PAYLOAD_BYTES: usize = 16;
pub(crate) const MAILBOX_CAP: usize = 256;

#[derive(Clone, Copy, Default)]
pub(crate) struct BusEndpointStats {
    pub(crate) sent: u64,
    pub(crate) received: u64,
    pub(crate) full: u64,
    pub(crate) drops: u64,
    pub(crate) max_depth: usize,
}

pub(crate) struct ProtectedMailboxEndpoint<const CAP: usize> {
    endpoint: EndpointId,
    inbox: Mailbox<CAP>,
    stats: BusEndpointStats,
}

impl<const CAP: usize> ProtectedMailboxEndpoint<CAP> {
    pub(crate) const fn new(endpoint: EndpointId) -> Self {
        Self {
            endpoint,
            inbox: Mailbox::new(),
            stats: BusEndpointStats {
                sent: 0,
                received: 0,
                full: 0,
                drops: 0,
                max_depth: 0,
            },
        }
    }

    pub(crate) fn reset(&mut self) {
        self.inbox.clear();
        self.stats = BusEndpointStats::default();
    }

    pub(crate) fn try_send(&mut self, message: Message) -> Result<(), MailboxError> {
        if message.to != self.endpoint {
            self.stats.drops += 1;
            return Err(MailboxError::Full(message));
        }

        match self.inbox.send(message) {
            Ok(()) => {
                self.stats.sent += 1;
                let depth = self.inbox.len();
                if depth > self.stats.max_depth {
                    self.stats.max_depth = depth;
                }
                Ok(())
            }
            Err(MailboxError::Full(message)) => {
                self.stats.full += 1;
                Err(MailboxError::Full(message))
            }
            Err(MailboxError::Empty) => Err(MailboxError::Empty),
        }
    }

    pub(crate) fn drain_one(&mut self) -> Result<Message, MailboxError> {
        let message = self.inbox.recv()?;
        self.stats.received += 1;
        Ok(message)
    }

    pub(crate) const fn stats(&self) -> BusEndpointStats {
        self.stats
    }
}

pub(crate) struct ShardBus {
    producer: ProtectedMailboxEndpoint<MAILBOX_CAP>,
    consumer: ProtectedMailboxEndpoint<MAILBOX_CAP>,
    control: ProtectedMailboxEndpoint<MAILBOX_CAP>,
    network: ProtectedMailboxEndpoint<MAILBOX_CAP>,
}

impl ShardBus {
    pub(crate) const fn new() -> Self {
        Self {
            producer: ProtectedMailboxEndpoint::new(PRODUCER_ENDPOINT),
            consumer: ProtectedMailboxEndpoint::new(CONSUMER_ENDPOINT),
            control: ProtectedMailboxEndpoint::new(CONTROL_ENDPOINT),
            network: ProtectedMailboxEndpoint::new(NETWORK_ENDPOINT),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.producer.reset();
        self.consumer.reset();
        self.control.reset();
        self.network.reset();
    }

    pub(crate) fn try_send(
        &mut self,
        dst: EndpointId,
        message: Message,
    ) -> Result<(), MailboxError> {
        if dst == PRODUCER_ENDPOINT {
            self.producer.try_send(message)
        } else if dst == CONSUMER_ENDPOINT {
            self.consumer.try_send(message)
        } else if dst == CONTROL_ENDPOINT {
            self.control.try_send(message)
        } else if dst == NETWORK_ENDPOINT {
            self.network.try_send(message)
        } else {
            Err(MailboxError::Full(message))
        }
    }

    pub(crate) fn drain_one(&mut self, endpoint: EndpointId) -> Result<Message, MailboxError> {
        if endpoint == PRODUCER_ENDPOINT {
            self.producer.drain_one()
        } else if endpoint == CONSUMER_ENDPOINT {
            self.consumer.drain_one()
        } else if endpoint == CONTROL_ENDPOINT {
            self.control.drain_one()
        } else if endpoint == NETWORK_ENDPOINT {
            self.network.drain_one()
        } else {
            Err(MailboxError::Empty)
        }
    }

    pub(crate) fn consumer_stats(&self) -> BusEndpointStats {
        self.consumer.stats()
    }

    pub(crate) fn control_stats(&self) -> BusEndpointStats {
        self.control.stats()
    }

    pub(crate) fn network_stats(&self) -> BusEndpointStats {
        self.network.stats()
    }
}

#[repr(align(4096))]
struct ProducerRegion(ProducerShard);

#[repr(align(4096))]
struct BusRegion(ShardBus);

pub(crate) struct ProducerShard {
    pub(crate) produced: u64,
    pub(crate) ack_count: u64,
    pub(crate) ack_batches: u64,
    pub(crate) last_ack_seq: u64,
}

impl ProducerShard {
    const fn new() -> Self {
        Self {
            produced: 0,
            ack_count: 0,
            ack_batches: 0,
            last_ack_seq: 0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

static mut PRODUCER_REGION: ProducerRegion = ProducerRegion(ProducerShard::new());
static mut BUS_REGION: BusRegion = BusRegion(ShardBus::new());

pub(crate) fn reset_regions() {
    with_producer_shard(|producer| producer.reset());
    with_bus(|bus| bus.reset());
    deny_regions();
}

pub(crate) fn with_producer_shard<R>(f: impl FnOnce(&mut ProducerShard) -> R) -> R {
    unsafe {
        let addr = addr_of_mut!(PRODUCER_REGION);
        mmu::allow_region(addr as usize, core::mem::size_of::<ProducerRegion>());
        let result = f(&mut (*addr).0);
        mmu::deny_region(addr as usize, core::mem::size_of::<ProducerRegion>());
        result
    }
}

pub(crate) fn with_bus<R>(f: impl FnOnce(&mut ShardBus) -> R) -> R {
    unsafe {
        let addr = addr_of_mut!(BUS_REGION);
        mmu::allow_region(addr as usize, core::mem::size_of::<BusRegion>());
        let result = f(&mut (*addr).0);
        mmu::deny_region(addr as usize, core::mem::size_of::<BusRegion>());
        result
    }
}

pub(crate) fn deny_regions() {
    unsafe {
        mmu::deny_region(
            addr_of_mut!(PRODUCER_REGION) as usize,
            core::mem::size_of::<ProducerRegion>(),
        );
        mmu::deny_region(
            addr_of_mut!(BUS_REGION) as usize,
            core::mem::size_of::<BusRegion>(),
        );
    }
}

pub(crate) fn bench_message(seq: u64) -> Message {
    let checksum = checksum(seq);
    let mut bytes = [0u8; dataplane_microkernel_core::MESSAGE_INLINE_BYTES];
    write_u64_le(&mut bytes[0..8], seq);
    write_u64_le(&mut bytes[8..16], checksum);
    Message::new(
        PRODUCER_TASK,
        CONSUMER_ENDPOINT,
        RequestId::new(seq as u32),
        BENCH_CAPABILITY,
        MessageBody::InlineBytes {
            len: BENCH_PAYLOAD_BYTES as u8,
            bytes,
        },
    )
}

pub(crate) fn validate_bench_message(message: Message) -> Result<u64, ()> {
    if message.from != PRODUCER_TASK || message.to != CONSUMER_ENDPOINT {
        return Err(());
    }
    if message.capability != BENCH_CAPABILITY {
        return Err(());
    }
    let MessageBody::InlineBytes { len, bytes } = message.body else {
        return Err(());
    };
    if len as usize != BENCH_PAYLOAD_BYTES {
        return Err(());
    }
    let seq = read_u64_le(&bytes[0..8]);
    let expected = checksum(seq);
    if read_u64_le(&bytes[8..16]) != expected {
        return Err(());
    }
    if message.request != RequestId::new(seq as u32) {
        return Err(());
    }
    Ok(seq)
}

pub(crate) fn network_status_message() -> Message {
    Message::new(
        NETWORK_TASK,
        CONTROL_ENDPOINT,
        SERVICE_REQUEST,
        SERVICE_CAPABILITY,
        MessageBody::Word(NETWORK_STATUS_OK),
    )
}

pub(crate) fn validate_network_status(message: Message) -> Result<(), ()> {
    if message.from != NETWORK_TASK
        || message.to != CONTROL_ENDPOINT
        || message.request != SERVICE_REQUEST
        || message.capability != SERVICE_CAPABILITY
        || message.body != MessageBody::Word(NETWORK_STATUS_OK)
    {
        return Err(());
    }
    Ok(())
}

pub(crate) fn control_ack_message() -> Message {
    Message::new(
        CONTROL_TASK,
        NETWORK_ENDPOINT,
        SERVICE_REQUEST,
        SERVICE_CAPABILITY,
        MessageBody::Word(CONTROL_ACK_OK),
    )
}

pub(crate) fn validate_control_ack(message: Message) -> Result<(), ()> {
    if message.from != CONTROL_TASK
        || message.to != NETWORK_ENDPOINT
        || message.request != SERVICE_REQUEST
        || message.capability != SERVICE_CAPABILITY
        || message.body != MessageBody::Word(CONTROL_ACK_OK)
    {
        return Err(());
    }
    Ok(())
}

pub(crate) const fn checksum(seq: u64) -> u64 {
    seq ^ 0x5a5a_a5a5_0123_4567
}

fn write_u64_le(dst: &mut [u8], value: u64) {
    let mut index = 0;
    while index < 8 {
        dst[index] = (value >> (index * 8)) as u8;
        index += 1;
    }
}

fn read_u64_le(src: &[u8]) -> u64 {
    let mut value = 0u64;
    let mut index = 0;
    while index < 8 {
        value |= (src[index] as u64) << (index * 8);
        index += 1;
    }
    value
}
