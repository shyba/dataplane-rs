use crate::reactor_driver::{DriverCapabilities, ReactorDriver, ReactorDriverWait};
use std::collections::VecDeque;

pub struct ReactorRuntime<D: ReactorDriver> {
    driver: D,
    ready_events: VecDeque<D::Event>,
}

impl<D: ReactorDriver> ReactorRuntime<D> {
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            ready_events: VecDeque::new(),
        }
    }

    pub fn into_inner(self) -> D {
        self.driver
    }

    pub fn driver(&self) -> &D {
        &self.driver
    }

    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    pub fn capabilities(&self) -> DriverCapabilities {
        self.driver.capabilities()
    }

    pub fn outstanding(&self) -> usize {
        self.driver.outstanding()
    }

    pub fn submit(&mut self, op: D::Submit, token: D::Token) -> Result<(), D::Error> {
        self.driver.submit(op, token)
    }


    pub fn flush(&mut self) -> Result<usize, D::Error> {
        self.driver.flush()
    }

    pub fn drain<F>(&mut self, max_events: usize, mut on_event: F) -> Result<usize, D::Error>
    where
        F: FnMut(D::Event),
    {
        let mut emitted = 0usize;

        while emitted < max_events {
            if let Some(event) = self.ready_events.pop_front() {
                on_event(event);
                emitted += 1;
                continue;
            }

            let remaining = max_events - emitted;
            let mut delivered = 0usize;
            self.driver.drain(usize::MAX, |event| {
                if delivered < remaining {
                    on_event(event);
                    delivered += 1;
                } else {
                    self.ready_events.push_back(event);
                }
            })?;

            if delivered == 0 {
                break;
            }
            emitted += delivered;
            if delivered < remaining {
                break;
            }
        }

        Ok(emitted)
    }
}

impl<D> ReactorRuntime<D>
where
    D: ReactorDriver + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
{
    #[inline(always)]
    pub fn drain_or_wait<F>(
        &mut self,
        max_events: usize,
        min_events: usize,
        on_event: F,
    ) -> Result<usize, <D as ReactorDriver>::Error>
    where
        F: FnMut(D::Event),
    {
        self.drain_or_wait_deadline(max_events, min_events, None, on_event)
    }

    /// Like [`ReactorRuntime::drain_or_wait`], but bounds the blocking wait
    /// by `timeout_ns` when given, so timer deadlines are honored while
    /// IO-idle.
    #[inline(always)]
    pub fn drain_or_wait_deadline<F>(
        &mut self,
        max_events: usize,
        min_events: usize,
        timeout_ns: Option<u64>,
        mut on_event: F,
    ) -> Result<usize, <D as ReactorDriver>::Error>
    where
        F: FnMut(D::Event),
    {
        let emitted = self.drain(max_events, &mut on_event)?;
        if emitted != 0 {
            return Ok(emitted);
        }

        self.driver.wait_deadline(min_events, timeout_ns)?;
        self.drain(max_events, on_event)
    }
}

#[cfg(test)]
mod tests {
    use super::ReactorRuntime;
    use crate::reactor_driver::{
        DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
    };
    use crate::reactor_model::{NetEvent, NetOp, OpToken};

    #[derive(Default)]
    struct DummyDriver {
        events: Vec<NetEvent>,
        submitted: Vec<(NetOp, OpToken)>,
        wait_calls: Vec<usize>,
    }

    impl ReactorDriver for DummyDriver {
        type Error = std::io::Error;
        type Token = OpToken;
        type Submit = NetOp;
        type Event = NetEvent;

        fn submit(&mut self, op: Self::Submit, token: Self::Token) -> Result<(), Self::Error> {
            self.submitted.push((op, token));
            Ok(())
        }

        fn flush(&mut self) -> Result<usize, Self::Error> {
            Ok(0)
        }

        fn drain<F>(&mut self, max_events: usize, mut on_event: F) -> Result<usize, Self::Error>
        where
            F: FnMut(Self::Event),
        {
            let mut count = 0;
            while count < max_events {
                match self.events.pop() {
                    Some(event) => {
                        on_event(event);
                        count += 1;
                    }
                    None => break,
                }
            }
            Ok(count)
        }

        fn outstanding(&self) -> usize {
            self.events.len()
        }

        fn capabilities(&self) -> DriverCapabilities {
            DriverCapabilities {
                backend: DriverBackendKind::Syscall,
                supports_accept_multi: false,
                supports_multishot: false,
                supports_fixed_buffers: false,
                supports_sqpoll: false,
            }
        }
    }

    impl ReactorDriverWait for DummyDriver {
        type Error = std::io::Error;
        type Readiness = ();

        fn readiness(&self) -> Option<Self::Readiness> {
            None
        }

        fn wait(&mut self, min_events: usize) -> Result<usize, Self::Error> {
            self.wait_calls.push(min_events);
            if self.events.is_empty() {
                self.events.push(NetEvent::OpComplete { token: OpToken(7), kind: crate::reactor_model::NetOpKind::Recv, result: 77, flags: 0 });
            }
            Ok(self.events.len())
        }

        fn wait_deadline(&mut self, min_events: usize, _timeout_ns: Option<u64>) -> Result<usize, Self::Error> {
            self.wait(min_events)
        }

    }


    #[test]
    fn drain_buffers_overflow_events() {
        let driver = DummyDriver {
            events: vec![
                NetEvent::OpComplete {
                    token: OpToken(1),
                    kind: crate::reactor_model::NetOpKind::Send,
                    result: 10,
                    flags: 0,
                },
                NetEvent::OpComplete {
                    token: OpToken(2),
                    kind: crate::reactor_model::NetOpKind::Recv,
                    result: 20,
                    flags: 0,
                },
            ],
            ..Default::default()
        };

        let mut runtime = ReactorRuntime::new(driver);
        let mut seen = Vec::new();

        let first = runtime.drain(1, |event| seen.push(event)).unwrap();
        assert_eq!(first, 1);
        assert_eq!(seen.len(), 1);
        assert_eq!(runtime.ready_events.len(), 1);

        let second = runtime.drain(1, |event| seen.push(event)).unwrap();
        assert_eq!(second, 1);
        assert_eq!(seen.len(), 2);
        assert_eq!(runtime.ready_events.len(), 0);
    }

    #[test]
    fn submit_forwards_token() {
        let driver = DummyDriver::default();
        let mut runtime = ReactorRuntime::new(driver);
        let token = OpToken(42);
        runtime.submit(NetOp::Send { fd: 0, ptr: std::ptr::null(), len: 0 }, token).unwrap();

        assert_eq!(runtime.driver().submitted.len(), 1);
        assert_eq!(runtime.driver().submitted[0].1, token);
    }

    #[test]
    fn drain_or_wait_drains_without_waiting_when_events_are_ready() {
        let mut driver = DummyDriver::default();
        driver.events.push(NetEvent::OpComplete {
            token: OpToken(1),
            kind: crate::reactor_model::NetOpKind::Send,
            result: 10,
            flags: 0,
        });

        let mut runtime = ReactorRuntime::new(driver);
        let mut seen = Vec::new();

        let emitted = runtime
            .drain_or_wait(1, 1, |event| seen.push(event))
            .unwrap();

        assert_eq!(emitted, 1);
        assert_eq!(seen.len(), 1);
        assert!(runtime.driver().wait_calls.is_empty());
    }

    #[test]
    fn drain_or_wait_waits_and_then_drains_when_initial_drain_is_empty() {
        let driver = DummyDriver::default();
        let mut runtime = ReactorRuntime::new(driver);
        let mut seen = Vec::new();

        let emitted = runtime
            .drain_or_wait(1, 2, |event| seen.push(event))
            .unwrap();

        assert_eq!(emitted, 1);
        assert_eq!(seen.len(), 1);
        assert_eq!(runtime.driver().wait_calls, vec![2]);
    }
}
