use super::UDP_ECHO_PAYLOAD_BYTES;

pub(crate) const CONTROL_PROTOCOL_V1_VERSION: u8 = 0;
pub(crate) const CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO: u8 = 0;
pub(crate) const CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES: usize = UDP_ECHO_PAYLOAD_BYTES;
pub(crate) const CONTROL_PROTOCOL_V1_REQUEST_MAGIC: &[u8; 8] = b"DPUDPQ00";
pub(crate) const CONTROL_PROTOCOL_V1_RESPONSE_MAGIC: &[u8; 8] = b"DPUDPR00";
pub(crate) const UDP_REQUEST_MAGIC: &[u8; 8] = CONTROL_PROTOCOL_V1_REQUEST_MAGIC;
pub(crate) const UDP_RESPONSE_MAGIC: &[u8; 8] = CONTROL_PROTOCOL_V1_RESPONSE_MAGIC;
pub(crate) const CONTROL_PROTOCOL_V1_ROUTE: ControlProtocolV1Route = ControlProtocolV1Route {
    version: CONTROL_PROTOCOL_V1_VERSION,
    opcode: CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO,
    max_payload_bytes: CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES,
    request_magic: UDP_REQUEST_MAGIC,
    response_magic: UDP_RESPONSE_MAGIC,
};

#[derive(Copy, Clone)]
pub(crate) enum ControlProtocolV1RejectReason {
    BadMagic,
    BadCheck,
    WrongRoute,
    WrongSrcPort,
    ShortPayload,
    OverlongPayload,
}

impl ControlProtocolV1RejectReason {
    fn marker(self) -> &'static str {
        match self {
            ControlProtocolV1RejectReason::BadMagic => "DPMK:CTRL-V1-REJECT:bad_magic",
            ControlProtocolV1RejectReason::BadCheck => "DPMK:CTRL-V1-REJECT:bad_check",
            ControlProtocolV1RejectReason::WrongRoute => "DPMK:CTRL-V1-REJECT:wrong_route",
            ControlProtocolV1RejectReason::WrongSrcPort => "DPMK:CTRL-V1-REJECT:wrong_src_port",
            ControlProtocolV1RejectReason::ShortPayload => "DPMK:CTRL-V1-REJECT:short_payload",
            ControlProtocolV1RejectReason::OverlongPayload => {
                "DPMK:CTRL-V1-REJECT:overlong_payload"
            }
        }
    }
}

pub(crate) fn reject_control_protocol_v1(reason: ControlProtocolV1RejectReason) {
    crate::serial::write_str(reason.marker());
    crate::serial::write_str("\n");
}

#[derive(Copy, Clone)]
pub(crate) struct ControlProtocolV1Route {
    pub(crate) version: u8,
    pub(crate) opcode: u8,
    pub(crate) max_payload_bytes: usize,
    pub(crate) request_magic: &'static [u8; 8],
    pub(crate) response_magic: &'static [u8; 8],
}
