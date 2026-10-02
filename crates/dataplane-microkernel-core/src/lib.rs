//! Fixed-capacity message, task-table, and network-frame vocabulary for smoke kernels.
//!
//! No allocator, hardware access, scheduling policy, or transport implementation
//! lives here. Constructors validate frame ranges; public wire-like descriptor
//! fields are not a substitute for validating untrusted input at a driver boundary.
//! Mailboxes return the original message on capacity exhaustion, including at zero capacity.
#![no_std]
#![forbid(unsafe_code)]

pub const MESSAGE_INLINE_BYTES: usize = 32;
pub const NETWORK_FRAME_MAX_BYTES: usize = 1522;
pub const ETHERNET_HEADER_BYTES: usize = 14;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u16);

impl TaskId {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointId(u16);

impl EndpointId {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityId(u16);

impl CapabilityId {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u32);

impl RequestId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetworkFrameBufferId(u16);

impl NetworkFrameBufferId {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkFrameDirection {
    Rx,
    Tx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkFrameType {
    Ethernet,
}

impl NetworkFrameType {
    pub const fn max_len(self) -> usize {
        match self {
            Self::Ethernet => NETWORK_FRAME_MAX_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkFrameError {
    EmptyFrame,
    FrameTooLarge {
        len: usize,
        max: usize,
    },
    BufferTooSmall {
        len: usize,
        capacity: usize,
    },
    FrameRangeOutOfBounds {
        offset: usize,
        len: usize,
        capacity: usize,
    },
    DescriptorValueTooLarge {
        value: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkFrameDescriptor {
    pub direction: NetworkFrameDirection,
    pub frame_type: NetworkFrameType,
    pub buffer: NetworkFrameBufferId,
    pub offset: u16,
    pub len: u16,
    pub capacity: u16,
}

impl NetworkFrameDescriptor {
    pub fn new(
        direction: NetworkFrameDirection,
        frame_type: NetworkFrameType,
        buffer: NetworkFrameBufferId,
        len: usize,
        capacity: usize,
    ) -> Result<Self, NetworkFrameError> {
        Self::with_offset(direction, frame_type, buffer, 0, len, capacity)
    }

    pub fn with_offset(
        direction: NetworkFrameDirection,
        frame_type: NetworkFrameType,
        buffer: NetworkFrameBufferId,
        offset: usize,
        len: usize,
        capacity: usize,
    ) -> Result<Self, NetworkFrameError> {
        if len == 0 && matches!(direction, NetworkFrameDirection::Tx) {
            return Err(NetworkFrameError::EmptyFrame);
        }
        let max = frame_type.max_len();
        if len > max {
            return Err(NetworkFrameError::FrameTooLarge { len, max });
        }
        if len > capacity {
            return Err(NetworkFrameError::BufferTooSmall { len, capacity });
        }
        let end = offset
            .checked_add(len)
            .ok_or(NetworkFrameError::DescriptorValueTooLarge { value: offset })?;
        if end > capacity {
            return Err(NetworkFrameError::FrameRangeOutOfBounds {
                offset,
                len,
                capacity,
            });
        }
        let offset = u16::try_from(offset)
            .map_err(|_| NetworkFrameError::DescriptorValueTooLarge { value: offset })?;
        let len = u16::try_from(len)
            .map_err(|_| NetworkFrameError::DescriptorValueTooLarge { value: len })?;
        let capacity = u16::try_from(capacity)
            .map_err(|_| NetworkFrameError::DescriptorValueTooLarge { value: capacity })?;

        Ok(Self {
            direction,
            frame_type,
            buffer,
            offset,
            len,
            capacity,
        })
    }

    pub const fn len(self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    pub const fn capacity(self) -> usize {
        self.capacity as usize
    }

    pub const fn is_rx(self) -> bool {
        matches!(self.direction, NetworkFrameDirection::Rx)
    }

    pub const fn is_tx(self) -> bool {
        matches!(self.direction, NetworkFrameDirection::Tx)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EthernetFrameSpec<'a> {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub ethertype: u16,
    pub payload: &'a [u8],
    pub pad: u8,
}

impl EthernetFrameSpec<'_> {
    pub const fn padded_len(self, min_len: usize) -> usize {
        let frame_len = ETHERNET_HEADER_BYTES + self.payload.len();
        if frame_len < min_len {
            min_len
        } else {
            frame_len
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceivedFrame {
    pub descriptor: NetworkFrameDescriptor,
    pub transport_len: u32,
}

pub trait FixedNetworkDriver<Memory, Error> {
    fn init(&mut self, memory: &mut Memory) -> Result<(), Error>;
    fn arm_receive(&mut self, memory: &mut Memory) -> Result<(), Error>;
    fn transmit_frame(
        &mut self,
        memory: &mut Memory,
        frame: EthernetFrameSpec<'_>,
    ) -> Result<(), Error>;
    fn receive_frame(&mut self, memory: &mut Memory) -> Result<ReceivedFrame, Error>;
}

pub struct FixedNetworkTask<D> {
    driver: D,
}

impl<D> FixedNetworkTask<D> {
    pub const fn new(driver: D) -> Self {
        Self { driver }
    }

    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    pub fn init<Memory, Error>(&mut self, memory: &mut Memory) -> Result<(), Error>
    where
        D: FixedNetworkDriver<Memory, Error>,
    {
        self.driver.init(memory)
    }

    pub fn arm_receive<Memory, Error>(&mut self, memory: &mut Memory) -> Result<(), Error>
    where
        D: FixedNetworkDriver<Memory, Error>,
    {
        self.driver.arm_receive(memory)
    }

    pub fn transmit_frame<Memory, Error>(
        &mut self,
        memory: &mut Memory,
        frame: EthernetFrameSpec<'_>,
    ) -> Result<(), Error>
    where
        D: FixedNetworkDriver<Memory, Error>,
    {
        self.driver.transmit_frame(memory, frame)
    }

    pub fn receive_frame<Memory, Error>(
        &mut self,
        memory: &mut Memory,
    ) -> Result<ReceivedFrame, Error>
    where
        D: FixedNetworkDriver<Memory, Error>,
    {
        self.driver.receive_frame(memory)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageBody {
    Empty,
    Word(u32),
    Pair(u32, u32),
    InlineBytes {
        len: u8,
        bytes: [u8; MESSAGE_INLINE_BYTES],
    },
}

impl MessageBody {
    pub const fn empty() -> Self {
        Self::Empty
    }

    pub fn inline_bytes(src: &[u8]) -> Result<Self, MessageError> {
        if src.len() > MESSAGE_INLINE_BYTES {
            return Err(MessageError::InlineBodyTooLarge);
        }

        let mut bytes = [0; MESSAGE_INLINE_BYTES];
        let mut index = 0;
        while index < src.len() {
            bytes[index] = src[index];
            index += 1;
        }

        Ok(Self::InlineBytes {
            len: src.len() as u8,
            bytes,
        })
    }

    pub const fn len(self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Word(_) => 4,
            Self::Pair(_, _) => 8,
            Self::InlineBytes { len, .. } => len as usize,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageError {
    InlineBodyTooLarge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub from: TaskId,
    pub to: EndpointId,
    pub request: RequestId,
    pub capability: CapabilityId,
    pub body: MessageBody,
}

impl Message {
    pub const fn new(
        from: TaskId,
        to: EndpointId,
        request: RequestId,
        capability: CapabilityId,
        body: MessageBody,
    ) -> Self {
        Self {
            from,
            to,
            request,
            capability,
            body,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxError {
    Full(Message),
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mailbox<const N: usize> {
    slots: [Option<Message>; N],
    head: usize,
    len: usize,
}

impl<const N: usize> Default for Mailbox<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Mailbox<N> {
    pub const fn new() -> Self {
        Self {
            slots: [None; N],
            head: 0,
            len: 0,
        }
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    pub fn enqueue(&mut self, message: Message) -> Result<(), MailboxError> {
        if self.is_full() {
            return Err(MailboxError::Full(message));
        }

        let tail = self.tail_index();
        self.slots[tail] = Some(message);
        self.len += 1;
        Ok(())
    }

    pub fn send(&mut self, message: Message) -> Result<(), MailboxError> {
        self.enqueue(message)
    }

    pub fn dequeue(&mut self) -> Result<Message, MailboxError> {
        if self.is_empty() {
            return Err(MailboxError::Empty);
        }

        let message = match self.slots[self.head].take() {
            Some(message) => message,
            None => return Err(MailboxError::Empty),
        };
        self.head = self.next_index(self.head);
        self.len -= 1;
        Ok(message)
    }

    pub fn recv(&mut self) -> Result<Message, MailboxError> {
        self.dequeue()
    }

    pub fn clear(&mut self) {
        while self.dequeue().is_ok() {}
        self.head = 0;
    }

    fn tail_index(&self) -> usize {
        if N == 0 {
            0
        } else {
            (self.head + self.len) % N
        }
    }

    fn next_index(&self, index: usize) -> usize {
        if N == 0 {
            0
        } else {
            (index + 1) % N
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TaskCounters {
    pub enqueued: u32,
    pub dequeued: u32,
    pub faults: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TaskStatus {
    #[default]
    Empty,
    Ready,
    Waiting,
    Faulted,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskSlot {
    pub id: TaskId,
    pub endpoint: EndpointId,
    pub status: TaskStatus,
    pub counters: TaskCounters,
}

impl Default for TaskSlot {
    fn default() -> Self {
        Self {
            id: TaskId::new(0),
            endpoint: EndpointId::new(0),
            status: TaskStatus::Empty,
            counters: TaskCounters::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskTableError {
    InvalidTask(TaskId),
    Occupied(TaskId),
    Empty(TaskId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskTable<const N: usize> {
    slots: [TaskSlot; N],
    active: usize,
}

impl<const N: usize> Default for TaskTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> TaskTable<N> {
    pub const fn new() -> Self {
        Self {
            slots: [TaskSlot {
                id: TaskId(0),
                endpoint: EndpointId(0),
                status: TaskStatus::Empty,
                counters: TaskCounters {
                    enqueued: 0,
                    dequeued: 0,
                    faults: 0,
                },
            }; N],
            active: 0,
        }
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub const fn active_count(&self) -> usize {
        self.active
    }

    pub fn insert(&mut self, id: TaskId, endpoint: EndpointId) -> Result<(), TaskTableError> {
        let index = self.index(id)?;
        if self.slots[index].status != TaskStatus::Empty {
            return Err(TaskTableError::Occupied(id));
        }

        self.slots[index] = TaskSlot {
            id,
            endpoint,
            status: TaskStatus::Ready,
            counters: TaskCounters::default(),
        };
        self.active += 1;
        Ok(())
    }

    pub fn remove(&mut self, id: TaskId) -> Result<TaskSlot, TaskTableError> {
        let index = self.index(id)?;
        if self.slots[index].status == TaskStatus::Empty {
            return Err(TaskTableError::Empty(id));
        }

        let removed = self.slots[index];
        self.slots[index] = TaskSlot::default();
        self.active -= 1;
        Ok(removed)
    }

    pub fn get(&self, id: TaskId) -> Result<&TaskSlot, TaskTableError> {
        let index = self.index(id)?;
        if self.slots[index].status == TaskStatus::Empty {
            return Err(TaskTableError::Empty(id));
        }
        Ok(&self.slots[index])
    }

    // Raw mutable access would let callers change Empty/id without updating
    // active_count. Expose state changes through the table's methods instead.
    fn get_mut(&mut self, id: TaskId) -> Result<&mut TaskSlot, TaskTableError> {
        let index = self.index(id)?;
        if self.slots[index].status == TaskStatus::Empty {
            return Err(TaskTableError::Empty(id));
        }
        Ok(&mut self.slots[index])
    }

    /// Change a live task's state. Setting `Empty` removes the task and updates
    /// the active count, exactly as [`Self::remove`] does.
    pub fn set_status(&mut self, id: TaskId, status: TaskStatus) -> Result<(), TaskTableError> {
        if status == TaskStatus::Empty {
            self.remove(id)?;
            return Ok(());
        }
        self.get_mut(id)?.status = status;
        Ok(())
    }

    pub fn record_enqueue(&mut self, id: TaskId) -> Result<(), TaskTableError> {
        let slot = self.get_mut(id)?;
        slot.counters.enqueued = slot.counters.enqueued.saturating_add(1);
        Ok(())
    }

    pub fn record_dequeue(&mut self, id: TaskId) -> Result<(), TaskTableError> {
        let slot = self.get_mut(id)?;
        slot.counters.dequeued = slot.counters.dequeued.saturating_add(1);
        Ok(())
    }

    pub fn record_fault(&mut self, id: TaskId) -> Result<(), TaskTableError> {
        let slot = self.get_mut(id)?;
        slot.counters.faults = slot.counters.faults.saturating_add(1);
        slot.status = TaskStatus::Faulted;
        Ok(())
    }

    fn index(&self, id: TaskId) -> Result<usize, TaskTableError> {
        let index = id.index();
        if index >= N {
            Err(TaskTableError::InvalidTask(id))
        } else {
            Ok(index)
        }
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;
