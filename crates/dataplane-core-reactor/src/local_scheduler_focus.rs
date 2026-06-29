use super::{LocalMeshScheduler, PushPolicy, TickReport, WorkDisposition};

pub trait FocusPolicy {
    fn run<Op, const STACK_BYTES: usize, F, Push>(
        scheduler: &mut LocalMeshScheduler<Op, STACK_BYTES, Self, Push>,
        on_work: &mut F,
        report: &mut TickReport,
    ) where
        Self: Sized,
        Push: PushPolicy,
        F: FnMut(&mut Op) -> WorkDisposition;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SingleTaskFocus;

impl FocusPolicy for SingleTaskFocus {
    fn run<Op, const STACK_BYTES: usize, F, Push>(
        scheduler: &mut LocalMeshScheduler<Op, STACK_BYTES, Self, Push>,
        on_work: &mut F,
        report: &mut TickReport,
    ) where
        Self: Sized,
        Push: PushPolicy,
        F: FnMut(&mut Op) -> WorkDisposition,
    {
        while report.local_executed < scheduler.config.run_budget {
            let Some(task_id) = scheduler.ready.pop_front() else {
                report.focus_exit_queue_empty = report.focus_exit_queue_empty.saturating_add(1);
                break;
            };
            let Some((action, local_polls, budget_requeue, time_requeue)) =
                scheduler.focus_task(task_id, on_work, report.local_executed)
            else {
                report.focus_exit_missing_task = report.focus_exit_missing_task.saturating_add(1);
                continue;
            };
            report.local_executed = report.local_executed.saturating_add(local_polls);
            match action {
                WorkDisposition::AllDone => {
                    report.focus_exit_all_done = report.focus_exit_all_done.saturating_add(1)
                }
                WorkDisposition::Complete => {
                    report.focus_exit_complete = report.focus_exit_complete.saturating_add(1)
                }
                WorkDisposition::DeferBus => {
                    report.focus_exit_defer_bus = report.focus_exit_defer_bus.saturating_add(1)
                }
                WorkDisposition::Requeue => {
                    if budget_requeue {
                        report.focus_exit_requeue_budget =
                            report.focus_exit_requeue_budget.saturating_add(1);
                    } else if time_requeue {
                        report.focus_exit_requeue_time =
                            report.focus_exit_requeue_time.saturating_add(1);
                    } else {
                        report.focus_exit_requeue_other =
                            report.focus_exit_requeue_other.saturating_add(1);
                    }
                }
            }
            scheduler.apply_task_action(task_id, action, report);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListSweepBudgetFocus;

impl FocusPolicy for ListSweepBudgetFocus {
    fn run<Op, const STACK_BYTES: usize, F, Push>(
        scheduler: &mut LocalMeshScheduler<Op, STACK_BYTES, Self, Push>,
        on_work: &mut F,
        report: &mut TickReport,
    ) where
        Self: Sized,
        Push: PushPolicy,
        F: FnMut(&mut Op) -> WorkDisposition,
    {
        scheduler.focus_round.clear();
        while let Some(id) = scheduler.ready.pop_front() {
            scheduler.focus_round.push(id);
        }

        while report.local_executed < scheduler.config.run_budget
            && !scheduler.focus_round.is_empty()
        {
            let mut idx = 0usize;
            while idx < scheduler.focus_round.len() {
                let task_id = scheduler.focus_round[idx];
                idx += 1;
                if report.local_executed >= scheduler.config.run_budget {
                    scheduler.ready.push_back(task_id);
                    continue;
                }
                let Some((action, local_polls, budget_requeue, time_requeue)) =
                    scheduler.focus_task(task_id, on_work, report.local_executed)
                else {
                    report.focus_exit_missing_task =
                        report.focus_exit_missing_task.saturating_add(1);
                    continue;
                };
                report.local_executed = report.local_executed.saturating_add(local_polls);
                match action {
                    WorkDisposition::AllDone => {
                        report.focus_exit_all_done = report.focus_exit_all_done.saturating_add(1)
                    }
                    WorkDisposition::Complete => {
                        report.focus_exit_complete = report.focus_exit_complete.saturating_add(1)
                    }
                    WorkDisposition::DeferBus => {
                        report.focus_exit_defer_bus = report.focus_exit_defer_bus.saturating_add(1)
                    }
                    WorkDisposition::Requeue => {
                        if budget_requeue {
                            report.focus_exit_requeue_budget =
                                report.focus_exit_requeue_budget.saturating_add(1);
                        } else if time_requeue {
                            report.focus_exit_requeue_time =
                                report.focus_exit_requeue_time.saturating_add(1);
                        } else {
                            report.focus_exit_requeue_other =
                                report.focus_exit_requeue_other.saturating_add(1);
                        }
                    }
                }
                scheduler.apply_task_action(task_id, action, report);
            }
            scheduler.focus_round.clear();

            if report.local_executed < scheduler.config.run_budget {
                while let Some(id) = scheduler.ready.pop_front() {
                    scheduler.focus_round.push(id);
                }
                if scheduler.focus_round.is_empty() {
                    report.focus_exit_queue_empty = report.focus_exit_queue_empty.saturating_add(1);
                }
            }
        }
        scheduler.focus_round.clear();
    }
}
