use crate::inflight_table::InflightTable;
use crate::reactor_driver::{ReactorDriver, ReactorDriverWait};
use crate::reactor_model::{OpToken, ReactorCompletion};
use crate::reactor_runtime::ReactorRuntime;

pub(crate) trait CompletionDrain<D>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
{
    fn drain_completion_batch_or_wait(
        &mut self,
        inflight: &mut InflightTable,
        max_events: usize,
        min_events: usize,
        timeout_ns: Option<u64>,
    ) -> Result<Vec<ReactorCompletion>, <D as ReactorDriver>::Error>;
}

impl<D> CompletionDrain<D> for ReactorRuntime<D>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
{
    fn drain_completion_batch_or_wait(
        &mut self,
        inflight: &mut InflightTable,
        max_events: usize,
        min_events: usize,
        timeout_ns: Option<u64>,
    ) -> Result<Vec<ReactorCompletion>, <D as ReactorDriver>::Error> {
        let mut completions = Vec::new();
        self.drain_or_wait_deadline(max_events, min_events, timeout_ns, |event| {
            let completion = ReactorCompletion::from(event);
            let _ = inflight.remove(completion.token);
            completions.push(completion);
        })?;
        Ok(completions)
    }
}
