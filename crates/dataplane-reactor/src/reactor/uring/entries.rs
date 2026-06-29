use io_uring::{cqueue, opcode, squeue, types};
use std::os::fd::RawFd;

pub fn cqe_more(flags: u32) -> bool {
    cqueue::more(flags)
}

pub fn accept_multi_entry(listener_fd: RawFd, flags: i32, token: u64) -> squeue::Entry {
    opcode::AcceptMulti::new(types::Fd(listener_fd))
        .flags(flags)
        .build()
        .user_data(token)
}

pub fn connect_entry(fd: RawFd, addr: &socket2::SockAddr, token: u64) -> squeue::Entry {
    opcode::Connect::new(types::Fd(fd), addr.as_ptr(), addr.len())
        .build()
        .user_data(token)
}

pub fn send_entry(fd: RawFd, ptr: *const u8, len: usize, token: u64) -> squeue::Entry {
    opcode::Send::new(types::Fd(fd), ptr, len as _)
        .build()
        .user_data(token)
}

pub fn recv_entry(fd: RawFd, ptr: *mut u8, len: usize, token: u64) -> squeue::Entry {
    opcode::Recv::new(types::Fd(fd), ptr, len as _)
        .build()
        .user_data(token)
}

pub fn recvmsg_entry(fd: RawFd, msg: *mut libc::msghdr, flags: i32, token: u64) -> squeue::Entry {
    opcode::RecvMsg::new(types::Fd(fd), msg)
        .flags((flags | libc::MSG_DONTWAIT) as _)
        .build()
        .user_data(token)
}
