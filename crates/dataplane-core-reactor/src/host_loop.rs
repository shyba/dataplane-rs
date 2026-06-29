mod completion;
mod facade;
mod task;

pub use facade::SubmissionFacade;
pub use task::HostLoop;

#[cfg(test)]
#[path = "host_loop_tests.rs"]
mod tests;
