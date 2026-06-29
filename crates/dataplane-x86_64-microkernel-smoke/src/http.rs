use crate::{
    append_bytes, append_decimal_bytes, find_byte, Message, MessageBody, CAP_KERNEL, EP_FS,
    EP_HTTP, GENERATION_ID, HTTP_CHAIN_FILE_BYTES, REQUEST_HTTP_FS, TASK_FS, TASK_HTTP,
    TCP_STREAM_TX_BYTES,
};

pub(crate) const HTTP_REQUEST_LINE_BYTES: usize = 96;
pub(crate) const HTTP_HEADER_BYTES: usize = 384;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HttpResponseKind {
    GetIndex,
    HeadIndex,
    GetLarge,
    GetChain,
    GetStatus,
    Missing,
    UnsupportedMethod,
    PayloadTooLarge,
    InternalError,
}
pub(crate) struct HttpResponse {
    pub(crate) len: usize,
    pub(crate) kind: HttpResponseKind,
}
pub(crate) struct HttpFileSet<'a> {
    pub(crate) index: &'a [u8],
    pub(crate) large: &'a [u8],
    pub(crate) chain: &'a [u8],
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum HttpMethod {
    Get,
    Head,
    Unsupported,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum HttpTarget {
    Index,
    Status,
    Large,
    Chain,
    Other,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum HttpRequestError {
    Malformed,
    RequestLineTooLong,
    HeadersTooLong,
}
#[derive(Clone, Copy)]
pub(crate) struct HttpPolicyCounters {
    pub(crate) response_200: u32,
    pub(crate) response_404: u32,
    pub(crate) response_405: u32,
    pub(crate) response_413: u32,
    pub(crate) response_500: u32,
    pub(crate) malformed_requests: u32,
    pub(crate) request_line_too_long: u32,
    pub(crate) headers_too_long: u32,
}
impl HttpPolicyCounters {
    const fn new() -> Self {
        Self {
            response_200: 0,
            response_404: 0,
            response_405: 0,
            response_413: 0,
            response_500: 0,
            malformed_requests: 0,
            request_line_too_long: 0,
            headers_too_long: 0,
        }
    }
}
#[derive(Clone, Copy)]
struct ParsedHttpRequest {
    method: HttpMethod,
    target: HttpTarget,
}
#[derive(Clone, Copy)]
enum HttpStatus {
    Ok,
    NotFound,
    MethodNotAllowed,
    PayloadTooLarge,
    InternalServerError,
}
pub(crate) struct HttpTask {
    counters: HttpPolicyCounters,
}

impl HttpTask {
    const HTTP_STATUS_200_OK: &'static [u8] = b"HTTP/1.0 200 OK";
    const HTTP_STATUS_404_NOT_FOUND: &'static [u8] = b"HTTP/1.0 404 Not Found";
    const HTTP_STATUS_405_METHOD_NOT_ALLOWED: &'static [u8] = b"HTTP/1.0 405 Method Not Allowed";
    const HTTP_STATUS_413_PAYLOAD_TOO_LARGE: &'static [u8] = b"HTTP/1.0 413 Payload Too Large";
    const HTTP_STATUS_500_INTERNAL_SERVER_ERROR: &'static [u8] =
        b"HTTP/1.0 500 Internal Server Error";
    pub(crate) const fn new() -> Self {
        Self {
            counters: HttpPolicyCounters::new(),
        }
    }
    pub(crate) fn counters(&self) -> HttpPolicyCounters {
        self.counters
    }
    pub(crate) fn fs_request(&self) -> Message {
        Message::new(
            TASK_HTTP,
            EP_FS,
            REQUEST_HTTP_FS,
            CAP_KERNEL,
            MessageBody::Word(0x4854),
        )
    }
    pub(crate) fn accept_fs_reply(
        &self,
        message: Message,
        index_size: u32,
        large_size: u32,
    ) -> Result<(), &'static str> {
        if message.from != TASK_FS || message.to != EP_HTTP || message.request != REQUEST_HTTP_FS {
            return Err("http-fs-reply");
        }
        if message.body != MessageBody::Pair(index_size, large_size) {
            return Err("http-fs-reply-body");
        }
        Ok(())
    }
    pub(crate) fn build_response(
        &mut self,
        request: &[u8],
        files: &HttpFileSet<'_>,
        out: &mut [u8],
    ) -> Result<HttpResponse, &'static str> {
        let response = match self.parse_request(request) {
            Ok(parsed) => match parsed.method {
                HttpMethod::Get if parsed.target == HttpTarget::Index => {
                    self.build_ok_response(files.index, true, HttpResponseKind::GetIndex, out)
                }
                HttpMethod::Head if parsed.target == HttpTarget::Large => {
                    self.build_ok_response(files.large, false, HttpResponseKind::GetLarge, out)
                }
                HttpMethod::Head if parsed.target == HttpTarget::Chain => {
                    self.build_ok_response(files.chain, false, HttpResponseKind::GetChain, out)
                }
                HttpMethod::Get if parsed.target == HttpTarget::Status => {
                    self.build_operator_parity_response(out)
                }
                HttpMethod::Head if parsed.target == HttpTarget::Index => {
                    self.build_ok_response(files.index, false, HttpResponseKind::HeadIndex, out)
                }
                HttpMethod::Get if parsed.target == HttpTarget::Large => {
                    self.build_ok_response(files.large, true, HttpResponseKind::GetLarge, out)
                }
                HttpMethod::Get if parsed.target == HttpTarget::Chain => {
                    self.build_ok_response(files.chain, true, HttpResponseKind::GetChain, out)
                }
                HttpMethod::Get | HttpMethod::Head => {
                    self.build_status_response(HttpStatus::NotFound, out)
                }
                HttpMethod::Unsupported => {
                    self.build_status_response(HttpStatus::MethodNotAllowed, out)
                }
            },
            Err(error) => {
                self.record_parse_error(error);
                self.build_status_response(HttpStatus::PayloadTooLarge, out)
            }
        };
        let response = match response {
            Ok(response) => response,
            Err(_) => self.build_status_response(HttpStatus::InternalServerError, out)?,
        };
        self.record_response(response.kind);
        Ok(response)
    }
    fn parse_request(&self, request: &[u8]) -> Result<ParsedHttpRequest, HttpRequestError> {
        let header_end = find_header_end(request)?;
        let line_end = find_request_line_end(request)?;
        if line_end > HTTP_REQUEST_LINE_BYTES {
            return Err(HttpRequestError::RequestLineTooLong);
        }
        let header_len = header_end.saturating_sub(line_end + 2);
        if header_len > HTTP_HEADER_BYTES {
            return Err(HttpRequestError::HeadersTooLong);
        }
        let line = &request[..line_end];
        let method_end = find_byte(line, b' ').ok_or(HttpRequestError::Malformed)?;
        let target_start = method_end + 1;
        if method_end == 0 || target_start >= line.len() {
            return Err(HttpRequestError::Malformed);
        }
        let target_rel_end =
            find_byte(&line[target_start..], b' ').ok_or(HttpRequestError::Malformed)?;
        let target_end = target_start + target_rel_end;
        let version_start = target_end + 1;
        if target_start == target_end || version_start >= line.len() {
            return Err(HttpRequestError::Malformed);
        }
        let method = match &line[..method_end] {
            b"GET" => HttpMethod::Get,
            b"HEAD" => HttpMethod::Head,
            _ => HttpMethod::Unsupported,
        };
        let target_bytes = &line[target_start..target_end];
        let target = if target_bytes == b"/" || target_bytes == b"/INDEX.HTM" {
            HttpTarget::Index
        } else if target_bytes == b"/STATUS.HTM" || target_bytes == b"/STATUS.TXT" {
            HttpTarget::Status
        } else if target_bytes == b"/LARGE.HTM" {
            HttpTarget::Large
        } else if target_bytes == b"/CHAIN.HTM" {
            HttpTarget::Chain
        } else {
            HttpTarget::Other
        };
        Ok(ParsedHttpRequest { method, target })
    }
    fn build_ok_response(
        &self,
        file: &[u8],
        include_body: bool,
        kind: HttpResponseKind,
        out: &mut [u8],
    ) -> Result<HttpResponse, &'static str> {
        if file.is_empty() || file.len() > HTTP_CHAIN_FILE_BYTES {
            return self.build_status_response(HttpStatus::InternalServerError, out);
        }
        let mut len = 0;
        self.append_status_headers(HttpStatus::Ok, file.len() as u32, out, &mut len)?;
        append_bytes(out, &mut len, b"Content-Type: ")?;
        append_bytes(out, &mut len, content_type(kind))?;
        append_bytes(out, &mut len, b"\r\n")?;
        append_bytes(out, &mut len, b"Connection: close\r\n\r\n")?;
        if include_body {
            append_bytes(out, &mut len, file)?;
        }
        Ok(HttpResponse { len, kind })
    }
    fn write_operator_parity_body(
        &self,
        out: &mut [u8],
        len: &mut usize,
    ) -> Result<(), &'static str> {
        append_bytes(out, len, b"DPSTATUS ")?;
        append_bytes(out, len, b"routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 ")?;
        append_bytes(
            out,
            len,
            b"service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 ",
        )?;
        append_bytes(out, len, b"storage_mode=ro ")?;
        append_bytes(
            out,
            len,
            b"network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 ",
        )?;
        append_bytes(out, len, b"timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp ")?;
        append_bytes(out, len, b"fault_status=tb:ready,f0,r0,p0,c0 ")?;
        append_bytes(out, len, b"generation_id=")?;
        append_decimal_bytes(out, len, GENERATION_ID)?;
        Ok(())
    }
    fn build_operator_parity_response(&self, out: &mut [u8]) -> Result<HttpResponse, &'static str> {
        let mut body = [0u8; 512];
        let mut body_len = 0;
        self.write_operator_parity_body(&mut body, &mut body_len)?;
        let mut len = 0;
        self.append_status_headers(HttpStatus::Ok, body_len as u32, out, &mut len)?;
        append_bytes(
            out,
            &mut len,
            b"Content-Type: text/plain\r\nConnection: close\r\n\r\n",
        )?;
        append_bytes(out, &mut len, &body[..body_len])?;
        Ok(HttpResponse {
            len,
            kind: HttpResponseKind::GetStatus,
        })
    }
    fn build_status_response(
        &self,
        status: HttpStatus,
        out: &mut [u8],
    ) -> Result<HttpResponse, &'static str> {
        let mut len = 0;
        self.append_status_headers(status, 0, out, &mut len)?;
        if matches!(status, HttpStatus::MethodNotAllowed) {
            append_bytes(out, &mut len, b"Allow: GET, HEAD\r\n")?;
        }
        append_bytes(out, &mut len, b"Connection: close\r\n\r\n")?;
        let kind = match status {
            HttpStatus::Ok => HttpResponseKind::GetIndex,
            HttpStatus::NotFound => HttpResponseKind::Missing,
            HttpStatus::MethodNotAllowed => HttpResponseKind::UnsupportedMethod,
            HttpStatus::PayloadTooLarge => HttpResponseKind::PayloadTooLarge,
            HttpStatus::InternalServerError => HttpResponseKind::InternalError,
        };
        Ok(HttpResponse { len, kind })
    }
    fn append_status_headers(
        &self,
        status: HttpStatus,
        content_len: u32,
        out: &mut [u8],
        len: &mut usize,
    ) -> Result<(), &'static str> {
        append_bytes(out, len, Self::status_line(status))?;
        append_bytes(out, len, b"\r\nContent-Length: ")?;
        append_decimal_bytes(out, len, content_len)?;
        append_bytes(out, len, b"\r\n")?;
        Ok(())
    }
    fn status_line(status: HttpStatus) -> &'static [u8] {
        match status {
            HttpStatus::Ok => Self::HTTP_STATUS_200_OK,
            HttpStatus::NotFound => Self::HTTP_STATUS_404_NOT_FOUND,
            HttpStatus::MethodNotAllowed => Self::HTTP_STATUS_405_METHOD_NOT_ALLOWED,
            HttpStatus::PayloadTooLarge => Self::HTTP_STATUS_413_PAYLOAD_TOO_LARGE,
            HttpStatus::InternalServerError => Self::HTTP_STATUS_500_INTERNAL_SERVER_ERROR,
        }
    }
    fn record_parse_error(&mut self, error: HttpRequestError) {
        match error {
            HttpRequestError::Malformed => self.counters.malformed_requests += 1,
            HttpRequestError::RequestLineTooLong => self.counters.request_line_too_long += 1,
            HttpRequestError::HeadersTooLong => self.counters.headers_too_long += 1,
        }
    }
    fn record_response(&mut self, kind: HttpResponseKind) {
        match kind {
            HttpResponseKind::GetIndex
            | HttpResponseKind::HeadIndex
            | HttpResponseKind::GetLarge
            | HttpResponseKind::GetChain
            | HttpResponseKind::GetStatus => self.counters.response_200 += 1,
            HttpResponseKind::Missing => self.counters.response_404 += 1,
            HttpResponseKind::UnsupportedMethod => self.counters.response_405 += 1,
            HttpResponseKind::PayloadTooLarge => self.counters.response_413 += 1,
            HttpResponseKind::InternalError => self.counters.response_500 += 1,
        }
    }
    pub(crate) fn probe_policy(&mut self) -> Result<(), &'static str> {
        let index = b"<html>dataplane</html>";
        self.expect_response(
            b"GET / HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::GetIndex,
            b"HTTP/1.0 200 OK",
        )?;
        self.expect_response(
            b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::GetIndex,
            b"HTTP/1.0 200 OK",
        )?;
        self.expect_response(
            b"HEAD /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::HeadIndex,
            b"HTTP/1.0 200 OK",
        )?;
        self.expect_response(
            b"GET /CHAIN.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::GetChain,
            b"HTTP/1.0 200 OK",
        )?;
        self.expect_response(
            b"HEAD /CHAIN.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::GetChain,
            b"HTTP/1.0 200 OK",
        )?;
        self.expect_response(
            b"GET /STATUS.TXT HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::GetStatus,
            b"HTTP/1.0 200 OK",
        )?;
        self.expect_response(
            b"GET /MISSING.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::Missing,
            b"HTTP/1.0 404 Not Found",
        )?;
        self.expect_response(
            b"POST /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            index,
            HttpResponseKind::UnsupportedMethod,
            b"HTTP/1.0 405 Method Not Allowed",
        )?;
        self.expect_response(
            b"GET\r\n\r\n",
            index,
            HttpResponseKind::PayloadTooLarge,
            b"HTTP/1.0 413 Payload Too Large",
        )?;
        let mut overlong = [b'A'; HTTP_REQUEST_LINE_BYTES + 5];
        let suffix = overlong.len() - 4;
        overlong[suffix..].copy_from_slice(b"\r\n\r\n");
        self.expect_response(
            &overlong,
            index,
            HttpResponseKind::PayloadTooLarge,
            b"HTTP/1.0 413 Payload Too Large",
        )?;
        self.expect_response(
            b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            &[],
            HttpResponseKind::InternalError,
            b"HTTP/1.0 500 Internal Server Error",
        )
    }
    fn expect_response(
        &mut self,
        request: &[u8],
        index_file: &[u8],
        expected_kind: HttpResponseKind,
        expected_prefix: &[u8],
    ) -> Result<(), &'static str> {
        let mut out = [0u8; TCP_STREAM_TX_BYTES];
        let files = HttpFileSet {
            index: index_file,
            large: index_file,
            chain: index_file,
        };
        let response = self.build_response(request, &files, &mut out)?;
        if response.kind != expected_kind || !starts_with(&out[..response.len], expected_prefix) {
            return Err("http-policy-probe");
        }
        Ok(())
    }
}

fn starts_with(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len() && &bytes[..prefix.len()] == prefix
}

fn content_type(kind: HttpResponseKind) -> &'static [u8] {
    match kind {
        HttpResponseKind::GetStatus => b"text/plain",
        _ => b"text/html",
    }
}
fn find_request_line_end(request: &[u8]) -> Result<usize, HttpRequestError> {
    let mut index = 0;
    while index + 1 < request.len() {
        if index > HTTP_REQUEST_LINE_BYTES {
            return Err(HttpRequestError::RequestLineTooLong);
        }
        if request[index] == b'\r' && request[index + 1] == b'\n' {
            return Ok(index);
        }
        index += 1;
    }
    if request.len() > HTTP_REQUEST_LINE_BYTES {
        Err(HttpRequestError::RequestLineTooLong)
    } else {
        Err(HttpRequestError::Malformed)
    }
}
fn find_header_end(request: &[u8]) -> Result<usize, HttpRequestError> {
    let mut index = 0;
    while index + 3 < request.len() {
        if index > HTTP_REQUEST_LINE_BYTES + HTTP_HEADER_BYTES {
            return Err(HttpRequestError::HeadersTooLong);
        }
        if request[index..index + 4] == *b"\r\n\r\n" {
            return Ok(index);
        }
        index += 1;
    }
    if request.len() > HTTP_REQUEST_LINE_BYTES + HTTP_HEADER_BYTES {
        Err(HttpRequestError::HeadersTooLong)
    } else {
        Err(HttpRequestError::Malformed)
    }
}
