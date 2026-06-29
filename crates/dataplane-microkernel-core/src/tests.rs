use super::*;

fn message(request: u32) -> Message {
    Message::new(
        TaskId::new(1),
        EndpointId::new(2),
        RequestId::new(request),
        CapabilityId::new(3),
        MessageBody::Word(request),
    )
}

#[test]
fn newtypes_round_trip_raw_values() {
    assert_eq!(TaskId::new(7).get(), 7);
    assert_eq!(EndpointId::new(8).get(), 8);
    assert_eq!(CapabilityId::new(9).get(), 9);
    assert_eq!(RequestId::new(10).get(), 10);
}

#[test]
fn inline_message_body_copies_without_heap() {
    let body = MessageBody::inline_bytes(b"hello").unwrap();
    match body {
        MessageBody::InlineBytes { len, bytes } => {
            assert_eq!(len, 5);
            assert_eq!(&bytes[..usize::from(len)], b"hello");
        }
        _ => panic!("unexpected body"),
    }
}

#[test]
fn oversized_inline_message_is_error() {
    let src = [0x55; MESSAGE_INLINE_BYTES + 1];
    assert_eq!(
        MessageBody::inline_bytes(&src),
        Err(MessageError::InlineBodyTooLarge)
    );
}

#[test]
fn network_frame_descriptor_records_driver_buffer_without_payload() {
    let descriptor = NetworkFrameDescriptor::new(
        NetworkFrameDirection::Rx,
        NetworkFrameType::Ethernet,
        NetworkFrameBufferId::new(4),
        64,
        128,
    )
    .unwrap();

    assert_eq!(descriptor.buffer.get(), 4);
    assert_eq!(descriptor.len(), 64);
    assert_eq!(descriptor.capacity(), 128);
    assert!(descriptor.is_rx());
    assert!(!descriptor.is_tx());
    assert!(!descriptor.is_empty());
}

#[test]
fn network_frame_descriptor_supports_offset_tx_slots() {
    let descriptor = NetworkFrameDescriptor::with_offset(
        NetworkFrameDirection::Tx,
        NetworkFrameType::Ethernet,
        NetworkFrameBufferId::new(7),
        16,
        60,
        NETWORK_FRAME_MAX_BYTES,
    )
    .unwrap();

    assert_eq!(descriptor.offset, 16);
    assert_eq!(descriptor.len(), 60);
    assert_eq!(descriptor.frame_type.max_len(), NETWORK_FRAME_MAX_BYTES);
    assert!(descriptor.is_tx());
}

#[test]
fn network_frame_descriptor_allows_posted_empty_rx_slots() {
    let descriptor = NetworkFrameDescriptor::new(
        NetworkFrameDirection::Rx,
        NetworkFrameType::Ethernet,
        NetworkFrameBufferId::new(2),
        0,
        NETWORK_FRAME_MAX_BYTES,
    )
    .unwrap();

    assert_eq!(descriptor.len(), 0);
    assert_eq!(descriptor.capacity(), NETWORK_FRAME_MAX_BYTES);
    assert!(descriptor.is_rx());
    assert!(descriptor.is_empty());
}

#[test]
fn network_frame_descriptor_rejects_invalid_sizes() {
    assert_eq!(
        NetworkFrameDescriptor::new(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NetworkFrameBufferId::new(1),
            0,
            128,
        ),
        Err(NetworkFrameError::EmptyFrame)
    );
    assert_eq!(
        NetworkFrameDescriptor::new(
            NetworkFrameDirection::Rx,
            NetworkFrameType::Ethernet,
            NetworkFrameBufferId::new(1),
            NETWORK_FRAME_MAX_BYTES + 1,
            NETWORK_FRAME_MAX_BYTES + 1,
        ),
        Err(NetworkFrameError::FrameTooLarge {
            len: NETWORK_FRAME_MAX_BYTES + 1,
            max: NETWORK_FRAME_MAX_BYTES,
        })
    );
    assert_eq!(
        NetworkFrameDescriptor::new(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NetworkFrameBufferId::new(1),
            256,
            128,
        ),
        Err(NetworkFrameError::BufferTooSmall {
            len: 256,
            capacity: 128,
        })
    );
    assert_eq!(
        NetworkFrameDescriptor::with_offset(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NetworkFrameBufferId::new(1),
            80,
            64,
            128,
        ),
        Err(NetworkFrameError::FrameRangeOutOfBounds {
            offset: 80,
            len: 64,
            capacity: 128,
        })
    );
}

#[test]
fn mailbox_fifo_and_capacity_accounting() {
    let mut mailbox = Mailbox::<2>::new();
    assert_eq!(mailbox.capacity(), 2);
    assert!(mailbox.is_empty());

    mailbox.enqueue(message(1)).unwrap();
    mailbox.enqueue(message(2)).unwrap();
    assert!(mailbox.is_full());
    assert_eq!(mailbox.len(), 2);

    assert_eq!(mailbox.dequeue().unwrap().request, RequestId::new(1));
    assert_eq!(mailbox.dequeue().unwrap().request, RequestId::new(2));
    assert!(mailbox.is_empty());
}

#[test]
fn mailbox_full_and_empty_are_regular_errors() {
    let mut mailbox = Mailbox::<1>::new();
    assert_eq!(mailbox.recv(), Err(MailboxError::Empty));

    mailbox.send(message(1)).unwrap();
    assert_eq!(
        mailbox.send(message(2)),
        Err(MailboxError::Full(message(2)))
    );
}

#[test]
fn zero_capacity_mailbox_reports_full_without_panic() {
    let mut mailbox = Mailbox::<0>::new();
    assert_eq!(mailbox.capacity(), 0);
    assert!(mailbox.is_full());
    assert_eq!(
        mailbox.enqueue(message(1)),
        Err(MailboxError::Full(message(1)))
    );
    assert_eq!(mailbox.dequeue(), Err(MailboxError::Empty));
}

#[test]
fn mailbox_wraps_ring_order() {
    let mut mailbox = Mailbox::<3>::new();
    mailbox.enqueue(message(1)).unwrap();
    mailbox.enqueue(message(2)).unwrap();
    assert_eq!(mailbox.dequeue().unwrap().request, RequestId::new(1));
    mailbox.enqueue(message(3)).unwrap();
    mailbox.enqueue(message(4)).unwrap();

    assert_eq!(mailbox.dequeue().unwrap().request, RequestId::new(2));
    assert_eq!(mailbox.dequeue().unwrap().request, RequestId::new(3));
    assert_eq!(mailbox.dequeue().unwrap().request, RequestId::new(4));
    assert_eq!(mailbox.dequeue(), Err(MailboxError::Empty));
}

#[test]
fn task_table_insert_status_and_counters() {
    let mut table = TaskTable::<4>::new();
    let id = TaskId::new(2);

    table.insert(id, EndpointId::new(9)).unwrap();
    assert_eq!(table.active_count(), 1);
    assert_eq!(table.get(id).unwrap().status, TaskStatus::Ready);

    table.record_enqueue(id).unwrap();
    table.record_dequeue(id).unwrap();
    table.record_fault(id).unwrap();

    let slot = table.get(id).unwrap();
    assert_eq!(slot.counters.enqueued, 1);
    assert_eq!(slot.counters.dequeued, 1);
    assert_eq!(slot.counters.faults, 1);
    assert_eq!(slot.status, TaskStatus::Faulted);
}

#[test]
fn task_table_rejects_invalid_and_occupied_slots() {
    let mut table = TaskTable::<1>::new();
    assert_eq!(
        table.insert(TaskId::new(1), EndpointId::new(1)),
        Err(TaskTableError::InvalidTask(TaskId::new(1)))
    );

    table.insert(TaskId::new(0), EndpointId::new(1)).unwrap();
    assert_eq!(
        table.insert(TaskId::new(0), EndpointId::new(2)),
        Err(TaskTableError::Occupied(TaskId::new(0)))
    );
}

#[test]
fn task_table_remove_clears_slot() {
    let mut table = TaskTable::<2>::new();
    let id = TaskId::new(1);

    table.insert(id, EndpointId::new(7)).unwrap();
    let removed = table.remove(id).unwrap();
    assert_eq!(removed.endpoint, EndpointId::new(7));
    assert_eq!(table.active_count(), 0);
    assert_eq!(table.get(id), Err(TaskTableError::Empty(id)));
}

struct LoopbackDriver {
    tx_seen: bool,
    rx_armed: bool,
}

impl FixedNetworkDriver<[u8; 64], NetworkFrameError> for LoopbackDriver {
    fn init(&mut self, _memory: &mut [u8; 64]) -> Result<(), NetworkFrameError> {
        Ok(())
    }

    fn arm_receive(&mut self, _memory: &mut [u8; 64]) -> Result<(), NetworkFrameError> {
        self.rx_armed = true;
        Ok(())
    }

    fn transmit_frame(
        &mut self,
        _memory: &mut [u8; 64],
        frame: EthernetFrameSpec<'_>,
    ) -> Result<(), NetworkFrameError> {
        NetworkFrameDescriptor::new(
            NetworkFrameDirection::Tx,
            NetworkFrameType::Ethernet,
            NetworkFrameBufferId::new(1),
            frame.padded_len(60),
            60,
        )?;
        self.tx_seen = true;
        Ok(())
    }

    fn receive_frame(
        &mut self,
        _memory: &mut [u8; 64],
    ) -> Result<ReceivedFrame, NetworkFrameError> {
        let descriptor = NetworkFrameDescriptor::new(
            NetworkFrameDirection::Rx,
            NetworkFrameType::Ethernet,
            NetworkFrameBufferId::new(2),
            60,
            64,
        )?;
        Ok(ReceivedFrame {
            descriptor,
            transport_len: 60,
        })
    }
}

#[test]
fn fixed_network_task_delegates_frame_level_rx_tx() {
    let mut memory = [0; 64];
    let mut task = FixedNetworkTask::new(LoopbackDriver {
        tx_seen: false,
        rx_armed: false,
    });
    let frame = EthernetFrameSpec {
        dst: [0xff; 6],
        src: [0x02; 6],
        ethertype: 0x88b5,
        payload: &[1, 2, 3],
        pad: 0,
    };

    task.init(&mut memory).unwrap();
    task.arm_receive(&mut memory).unwrap();
    task.transmit_frame(&mut memory, frame).unwrap();
    let received = task.receive_frame(&mut memory).unwrap();

    assert!(task.driver_mut().rx_armed);
    assert!(task.driver_mut().tx_seen);
    assert!(received.descriptor.is_rx());
    assert_eq!(received.transport_len, 60);
}
