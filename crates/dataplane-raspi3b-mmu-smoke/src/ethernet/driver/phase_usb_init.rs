use super::*;

pub(super) fn poll_probe_core(task: &mut UsbEthernetTask, _region: &mut [u8; SHARD_REGION_SIZE]) {
    task.core_id = read_reg(GSNPSID);
    write_reg(
        GAHBCFG,
        read_reg(GAHBCFG) | GAHBCFG_GLBL_INTR_EN | GAHBCFG_DMA_EN,
    );
    task.state = UsbEthernetState::PowerPort;
}

pub(super) fn poll_power_port(task: &mut UsbEthernetTask, _region: &mut [u8; SHARD_REGION_SIZE]) {
    let hprt = read_reg(HPRT);
    write_reg(HPRT, hprt_write_value(hprt) | HPRT_POWER | HPRT_RESET);
    task.state = UsbEthernetState::ReleaseReset;
}

pub(super) fn poll_release_reset(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    let hprt = read_reg(HPRT);
    write_reg(HPRT, hprt_write_value(hprt) | HPRT_POWER);
    task.state = UsbEthernetState::WaitEnabled;
}

pub(super) fn poll_wait_enabled(
    task: &mut UsbEthernetTask,
    _region: &mut [u8; SHARD_REGION_SIZE],
) {
    task.hprt = read_reg(HPRT);
    if task.hprt & (HPRT_CONN | HPRT_ENABLE) == (HPRT_CONN | HPRT_ENABLE) {
        task.state = UsbEthernetState::StartDescriptorSetup;
    } else if task.hprt & HPRT_CONN == 0 {
        task.state = UsbEthernetState::Failed;
        task.error = task.hprt;
    }
}
