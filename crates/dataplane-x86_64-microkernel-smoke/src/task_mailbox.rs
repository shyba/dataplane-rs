use crate::layout::TASK_TABLE_SLOTS;
use crate::services::SERVICE_MAILBOX_CAP;
use dataplane_microkernel_core::{Mailbox, Message, TaskId, TaskTable};

pub(crate) fn send_to_task(
    tasks: &mut TaskTable<TASK_TABLE_SLOTS>,
    task: TaskId,
    mailbox: &mut Mailbox<SERVICE_MAILBOX_CAP>,
    message: Message,
) -> Result<(), &'static str> {
    mailbox.send(message).map_err(|_| "mailbox-send")?;
    tasks.record_enqueue(task).map_err(|_| "task-enqueue")
}

pub(crate) fn recv_from_task(
    tasks: &mut TaskTable<TASK_TABLE_SLOTS>,
    task: TaskId,
    mailbox: &mut Mailbox<SERVICE_MAILBOX_CAP>,
) -> Result<Message, &'static str> {
    let message = mailbox.recv().map_err(|_| "mailbox-recv")?;
    tasks.record_dequeue(task).map_err(|_| "task-dequeue")?;
    Ok(message)
}
