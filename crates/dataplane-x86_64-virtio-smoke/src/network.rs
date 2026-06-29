use dataplane_microkernel_core::{
    EthernetFrameSpec, FixedNetworkDriver, FixedNetworkTask, NetworkFrameBufferId,
    ETHERNET_HEADER_BYTES,
};

pub(crate) const NET_TASK_BYTES: usize = 64 * 1024;
pub(crate) const RX_BUFFER_OFFSET: usize = 32 * 1024;
pub(crate) const TX_BUFFER_OFFSET: usize = 36 * 1024;

pub(crate) const VIRTIO_NET_HDR_LEN: usize = 12;
pub(crate) const TX_FRAME_LEN: usize = 60;
pub(crate) const TX_PACKET_LEN: usize = VIRTIO_NET_HDR_LEN + TX_FRAME_LEN;

pub(crate) const VM_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
pub(crate) const HOST_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x04];

#[cfg(not(feature = "udp-bench"))]
pub(crate) const HOST_CHALLENGE_PAYLOAD: &[u8] = b"DPHOST-CHALLENGE-0001";
#[cfg(not(feature = "udp-bench"))]
pub(crate) const VM_RESPONSE_PAYLOAD: &[u8] = b"DPX86-RESPONSE-OK-0001";

#[cfg(feature = "udp-bench")]
pub(crate) const UDP_BENCH_PACKETS: u32 = 128;
#[cfg(feature = "udp-bench")]
pub(crate) const UDP_BENCH_PAYLOAD_LEN: usize = 16;
#[cfg(feature = "udp-bench")]
pub(crate) const IPV4_UDP_PACKET_LEN: usize = 20 + 8 + UDP_BENCH_PAYLOAD_LEN;
#[cfg(feature = "udp-bench")]
pub(crate) const HOST_IPV4: [u8; 4] = [10, 0, 0, 1];
#[cfg(feature = "udp-bench")]
pub(crate) const VM_IPV4: [u8; 4] = [10, 0, 0, 2];
#[cfg(feature = "udp-bench")]
pub(crate) const HOST_UDP_PORT: u16 = 40_000;
#[cfg(feature = "udp-bench")]
pub(crate) const VM_UDP_PORT: u16 = 40_001;
#[cfg(feature = "udp-bench")]
pub(crate) const UDP_REQUEST_MAGIC: &[u8; 8] = b"DPUDPQ00";
#[cfg(feature = "udp-bench")]
pub(crate) const UDP_RESPONSE_MAGIC: &[u8; 8] = b"DPUDPR00";

pub(crate) const NET_RX_BUFFER_ID: NetworkFrameBufferId = NetworkFrameBufferId::new(1);
pub(crate) const NET_TX_BUFFER_ID: NetworkFrameBufferId = NetworkFrameBufferId::new(2);

pub(crate) struct NetworkTaskMemory<'a> {
    bytes: &'a mut [u8; NET_TASK_BYTES],
}

impl<'a> NetworkTaskMemory<'a> {
    pub(crate) fn new(bytes: &'a mut [u8; NET_TASK_BYTES]) -> Self {
        Self { bytes }
    }

    pub(crate) fn as_mut_bytes(&mut self) -> &mut [u8; NET_TASK_BYTES] {
        self.bytes
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FailReason(pub(crate) &'static str);

impl FailReason {
    pub(crate) fn as_str(self) -> &'static str {
        self.0
    }
}

pub(crate) enum TaskOutcome {
    VmCommunication,
    Failed(FailReason),
}

pub(crate) struct NetworkDriverTask<D> {
    fixed: FixedNetworkTask<D>,
}

impl<D> NetworkDriverTask<D>
where
    for<'a> D: FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason>,
{
    pub(crate) const fn new(fixed: FixedNetworkTask<D>) -> Self {
        Self { fixed }
    }

    pub(crate) fn run(&mut self, memory: &mut NetworkTaskMemory<'_>) -> TaskOutcome {
        if let Err(err) = self.fixed.init(memory) {
            return TaskOutcome::Failed(err);
        }
        if let Err(err) = self.fixed.transmit_frame(memory, tx_probe_frame()) {
            return TaskOutcome::Failed(err);
        }

        #[cfg(feature = "udp-bench")]
        if let Err(err) = self.run_udp_bench(memory) {
            return TaskOutcome::Failed(err);
        }

        #[cfg(not(feature = "udp-bench"))]
        if let Err(err) = self.run_host_challenge(memory) {
            return TaskOutcome::Failed(err);
        }

        TaskOutcome::VmCommunication
    }

    #[cfg(not(feature = "udp-bench"))]
    fn run_host_challenge(&mut self, memory: &mut NetworkTaskMemory<'_>) -> Result<(), FailReason> {
        let received = self.fixed.receive_frame(memory)?;
        validate_host_challenge(memory.as_mut_bytes(), received.transport_len)?;
        self.fixed.transmit_frame(memory, host_response_frame())
    }

    #[cfg(feature = "udp-bench")]
    fn run_udp_bench(&mut self, memory: &mut NetworkTaskMemory<'_>) -> Result<(), FailReason> {
        let mut completed = 0;
        while completed < UDP_BENCH_PACKETS {
            let received = self.fixed.receive_frame(memory)?;
            let request =
                match validate_udp_bench_request(memory.as_mut_bytes(), received.transport_len) {
                    Ok(request) => request,
                    Err(err) if err.as_str() == "udp-not-request" => {
                        self.fixed.arm_receive(memory)?;
                        continue;
                    }
                    Err(err) => return Err(err),
                };

            if request.seq < completed {
                self.fixed.arm_receive(memory)?;
                continue;
            }
            if request.seq != completed {
                return Err(FailReason("udp-seq"));
            }

            self.fixed.arm_receive(memory)?;
            let mut packet = [0; IPV4_UDP_PACKET_LEN];
            prepare_udp_bench_response_packet(&mut packet, completed);
            self.fixed.transmit_frame(
                memory,
                EthernetFrameSpec {
                    dst: HOST_MAC,
                    src: VM_MAC,
                    ethertype: 0x0800,
                    payload: &packet,
                    pad: 0,
                },
            )?;
            completed += 1;
        }
        Ok(())
    }
}

pub(crate) fn tx_probe_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: [0xff; 6],
        src: [0x02, 0x00, 0x00, 0x00, 0x00, 0x03],
        ethertype: 0x88b7,
        payload: &[
            14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
            36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57,
            58, 59,
        ],
        pad: 0,
    }
}

#[cfg(not(feature = "udp-bench"))]
pub(crate) fn host_response_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: HOST_MAC,
        src: VM_MAC,
        ethertype: 0x88b9,
        payload: VM_RESPONSE_PAYLOAD,
        pad: 0xa5,
    }
}

pub(crate) fn make_tx_frame(
    region: &mut [u8; NET_TASK_BYTES],
    frame_spec: EthernetFrameSpec<'_>,
) -> Result<(), FailReason> {
    if frame_spec.payload.len() > TX_FRAME_LEN - ETHERNET_HEADER_BYTES {
        return Err(FailReason("tx-payload-len"));
    }

    zero_region(region, TX_BUFFER_OFFSET, TX_PACKET_LEN);
    let frame = TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
    region[frame..frame + 6].copy_from_slice(&frame_spec.dst);
    region[frame + 6..frame + 12].copy_from_slice(&frame_spec.src);
    region[frame + 12] = (frame_spec.ethertype >> 8) as u8;
    region[frame + 13] = frame_spec.ethertype as u8;
    let payload = frame + ETHERNET_HEADER_BYTES;
    region[payload..payload + frame_spec.payload.len()].copy_from_slice(frame_spec.payload);
    for index in ETHERNET_HEADER_BYTES + frame_spec.payload.len()..TX_FRAME_LEN {
        region[frame + index] = frame_spec.pad;
    }
    Ok(())
}

#[cfg(not(feature = "udp-bench"))]
pub(crate) fn validate_host_challenge(
    region: &[u8; NET_TASK_BYTES],
    len: u32,
) -> Result<(), FailReason> {
    if len as usize > 2048 || (len as usize) < VIRTIO_NET_HDR_LEN + TX_FRAME_LEN {
        return Err(FailReason("rx-len"));
    }

    let frame = RX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
    let frame = if host_challenge_matches_at(region, frame) {
        frame
    } else if len as usize >= VIRTIO_NET_HDR_LEN + TX_FRAME_LEN + 2
        && region[frame] == 0
        && region[frame + 1] == 0
        && host_challenge_matches_at(region, frame + 2)
    {
        frame + 2
    } else {
        return Err(FailReason("rx-not-challenge"));
    };

    if region[frame..frame + 6] != VM_MAC {
        return Err(FailReason("rx-dst-mac"));
    }
    if region[frame + 6..frame + 12] != HOST_MAC {
        return Err(FailReason("rx-src-mac"));
    }
    if region[frame + 12] != 0x88 || region[frame + 13] != 0xb8 {
        return Err(FailReason("rx-not-challenge"));
    }
    let payload = frame + 14;
    if region[payload..payload + HOST_CHALLENGE_PAYLOAD.len()] != *HOST_CHALLENGE_PAYLOAD {
        return Err(FailReason("rx-challenge-payload"));
    }
    Ok(())
}

#[cfg(not(feature = "udp-bench"))]
fn host_challenge_matches_at(region: &[u8; NET_TASK_BYTES], frame: usize) -> bool {
    let payload = frame + 14;
    region[frame..frame + 6] == VM_MAC
        && region[frame + 6..frame + 12] == HOST_MAC
        && region[frame + 12] == 0x88
        && region[frame + 13] == 0xb8
        && region[payload..payload + HOST_CHALLENGE_PAYLOAD.len()] == *HOST_CHALLENGE_PAYLOAD
}

#[cfg(feature = "udp-bench")]
#[derive(Clone, Copy)]
struct UdpBenchRequest {
    seq: u32,
}

#[cfg(feature = "udp-bench")]
pub(crate) fn validate_udp_bench_request(
    region: &[u8; NET_TASK_BYTES],
    len: u32,
) -> Result<UdpBenchRequest, FailReason> {
    let frame = udp_bench_frame_offset(region, len)?;
    let packet = frame + 14;
    let udp = packet + 20;
    let payload = udp + 8;

    if frame + 14 + IPV4_UDP_PACKET_LEN > RX_BUFFER_OFFSET + len as usize {
        return Err(FailReason("udp-rx-len"));
    }
    if region[frame..frame + 6] != VM_MAC {
        return Err(FailReason("udp-dst-mac"));
    }
    if region[frame + 6..frame + 12] != HOST_MAC {
        return Err(FailReason("udp-src-mac"));
    }
    if region[frame + 12] != 0x08 || region[frame + 13] != 0x00 {
        return Err(FailReason("udp-ethertype"));
    }
    if region[packet] != 0x45 || region[packet + 9] != 17 {
        return Err(FailReason("udp-ipv4"));
    }
    if read_be_u16(&region[packet + 2..packet + 4]) != IPV4_UDP_PACKET_LEN as u16 {
        return Err(FailReason("udp-ip-len"));
    }
    if ipv4_header_checksum(&region[packet..packet + 20]) != 0 {
        return Err(FailReason("udp-ip-checksum"));
    }
    if region[packet + 12..packet + 16] != HOST_IPV4 {
        return Err(FailReason("udp-src-ip"));
    }
    if region[packet + 16..packet + 20] != VM_IPV4 {
        return Err(FailReason("udp-dst-ip"));
    }
    if read_be_u16(&region[udp..udp + 2]) != HOST_UDP_PORT {
        return Err(FailReason("udp-src-port"));
    }
    if read_be_u16(&region[udp + 2..udp + 4]) != VM_UDP_PORT {
        return Err(FailReason("udp-dst-port"));
    }
    if read_be_u16(&region[udp + 4..udp + 6]) != (8 + UDP_BENCH_PAYLOAD_LEN) as u16 {
        return Err(FailReason("udp-len"));
    }
    if region[payload..payload + UDP_REQUEST_MAGIC.len()] != *UDP_REQUEST_MAGIC {
        return Err(FailReason("udp-magic"));
    }

    let seq = read_be_u32(&region[payload + 8..payload + 12]);
    let check = read_be_u32(&region[payload + 12..payload + 16]);
    if check != (seq ^ 0xa5a5_a5a5) {
        return Err(FailReason("udp-check"));
    }
    Ok(UdpBenchRequest { seq })
}

#[cfg(feature = "udp-bench")]
fn udp_bench_frame_offset(region: &[u8; NET_TASK_BYTES], len: u32) -> Result<usize, FailReason> {
    let len = len as usize;
    let base = RX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
    let min_len = VIRTIO_NET_HDR_LEN + 14 + IPV4_UDP_PACKET_LEN;
    if len > 2048 || len < min_len {
        return Err(FailReason("udp-rx-len"));
    }
    if udp_bench_ether_header_matches_at(region, base) {
        return Ok(base);
    }
    if len >= min_len + 2
        && region[base] == 0
        && region[base + 1] == 0
        && udp_bench_ether_header_matches_at(region, base + 2)
    {
        return Ok(base + 2);
    }
    Err(FailReason("udp-not-request"))
}

#[cfg(feature = "udp-bench")]
fn udp_bench_ether_header_matches_at(region: &[u8; NET_TASK_BYTES], frame: usize) -> bool {
    region[frame..frame + 6] == VM_MAC
        && region[frame + 6..frame + 12] == HOST_MAC
        && region[frame + 12] == 0x08
        && region[frame + 13] == 0x00
}

#[cfg(feature = "udp-bench")]
pub(crate) fn prepare_udp_bench_response_packet(packet: &mut [u8; IPV4_UDP_PACKET_LEN], seq: u32) {
    for byte in packet.iter_mut() {
        *byte = 0;
    }

    packet[0] = 0x45;
    packet[1] = 0;
    write_be_u16(&mut packet[2..4], IPV4_UDP_PACKET_LEN as u16);
    write_be_u16(&mut packet[4..6], seq as u16);
    write_be_u16(&mut packet[6..8], 0x4000);
    packet[8] = 64;
    packet[9] = 17;
    packet[12..16].copy_from_slice(&VM_IPV4);
    packet[16..20].copy_from_slice(&HOST_IPV4);
    let checksum = ipv4_header_checksum(&packet[..20]);
    write_be_u16(&mut packet[10..12], checksum);

    let udp = 20;
    write_be_u16(&mut packet[udp..udp + 2], VM_UDP_PORT);
    write_be_u16(&mut packet[udp + 2..udp + 4], HOST_UDP_PORT);
    write_be_u16(
        &mut packet[udp + 4..udp + 6],
        (8 + UDP_BENCH_PAYLOAD_LEN) as u16,
    );
    write_be_u16(&mut packet[udp + 6..udp + 8], 0);

    let payload = udp + 8;
    packet[payload..payload + UDP_RESPONSE_MAGIC.len()].copy_from_slice(UDP_RESPONSE_MAGIC);
    write_be_u32(&mut packet[payload + 8..payload + 12], seq);
    write_be_u32(&mut packet[payload + 12..payload + 16], seq ^ 0x5a5a_5a5a);
}

#[cfg(feature = "udp-bench")]
fn ipv4_header_checksum(header: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut index = 0;
    while index < 20 {
        sum += u32::from(read_be_u16(&header[index..index + 2]));
        index += 2;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(feature = "udp-bench")]
fn write_be_u16(dst: &mut [u8], value: u16) {
    dst[0] = (value >> 8) as u8;
    dst[1] = value as u8;
}

#[cfg(feature = "udp-bench")]
fn write_be_u32(dst: &mut [u8], value: u32) {
    dst[0] = (value >> 24) as u8;
    dst[1] = (value >> 16) as u8;
    dst[2] = (value >> 8) as u8;
    dst[3] = value as u8;
}

#[cfg(feature = "udp-bench")]
fn read_be_u16(src: &[u8]) -> u16 {
    (u16::from(src[0]) << 8) | u16::from(src[1])
}

#[cfg(feature = "udp-bench")]
fn read_be_u32(src: &[u8]) -> u32 {
    (u32::from(src[0]) << 24)
        | (u32::from(src[1]) << 16)
        | (u32::from(src[2]) << 8)
        | u32::from(src[3])
}

fn zero_region(region: &mut [u8; NET_TASK_BYTES], offset: usize, len: usize) {
    for byte in &mut region[offset..offset + len] {
        *byte = 0;
    }
}
