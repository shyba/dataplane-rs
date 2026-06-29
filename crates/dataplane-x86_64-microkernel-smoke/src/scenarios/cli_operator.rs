use crate::*;

impl KernelState {
    pub(crate) fn run_integrated_fairness(&mut self) -> Result<(), &'static str> {
        serial::write_str("DPMK:FAIR-BEGIN\n");
        serial::write_str("DPMK:FAIR-CLI-INPUT-READY\n");

        let mut line = [0; CLI_LINE_BYTES];
        let mut first_command = [0u8; CLI_LINE_BYTES];
        let first_command_len =
            match serial::read_line_with_timeout(&mut line, CLI_FIRST_COMMAND_TIMEOUT_SPINS)? {
                Some(command) => {
                    let mut index = 0;
                    while index < command.len() {
                        first_command[index] = command[index];
                        index += 1;
                    }
                    command.len()
                }
                None => {
                    serial::write_str("DPMK:FAIR-SKIPPED\n");
                    return Ok(());
                }
            };

        let mut http_served = 0u32;
        let mut cli_served = 0u32;
        let mut fs_reads = 0u32;
        let mut timer_ticks = 0u32;
        let mut net_polls = 0u32;
        let mut work_units = 0u32;
        let mut last_timer_unit = 0u32;
        let mut max_gap = 0u32;
        let mut operator_done = false;
        let mut saw_tasks = false;
        let mut saw_ls = false;
        let mut saw_cat_index = false;

        let mut round = 0u32;
        while round < STAGE_G_ROUNDS {
            round += 1;

            self.run_timer_tick()?;
            timer_ticks += 1;
            let gap = work_units - last_timer_unit;
            if gap > max_gap {
                max_gap = gap;
            }
            last_timer_unit = work_units;

            let mut index_file = [0u8; HTTP_INDEX_FILE_BYTES];
            let mut large_file = [0u8; HTTP_LARGE_FILE_BYTES];
            let mut chain_file = [0u8; HTTP_CHAIN_FILE_BYTES];
            let (index_len, _, _) =
                self.load_http_files(&mut index_file, &mut large_file, &mut chain_file)?;
            fs_reads += 1;
            work_units += 1;

            let mut attempts = 0u32;
            let mut served_http_this_round = false;
            while attempts < 512 && !served_http_this_round {
                attempts += 1;
                let http_files = HttpFileSet {
                    index: &index_file[..index_len],
                    large: &large_file[..0],
                    chain: &chain_file[..0],
                };
                match self.fairness_poll_network(&http_files)? {
                    Some(NetworkReplyKind::HttpGet) => {
                        http_served += 1;
                        net_polls += 1;
                        work_units += 1;
                        served_http_this_round = true;
                    }
                    Some(_) => {
                        net_polls += 1;
                        work_units += 1;
                    }
                    None => {}
                }
                if attempts.is_multiple_of(16) {
                    self.run_timer_tick()?;
                    timer_ticks += 1;
                    let gap = work_units - last_timer_unit;
                    if gap > max_gap {
                        max_gap = gap;
                    }
                    last_timer_unit = work_units;
                }
            }
            if !served_http_this_round {
                return Err("fair-http-timeout");
            }

            let command = if round == 1 {
                &first_command[..first_command_len]
            } else {
                serial::read_line(&mut line)?
            };
            serial::write_str("DPMK:FAIR-CLI-BEGIN:");
            serial::write_decimal(round);
            serial::write_str(":");
            serial::write_bytes(command);
            serial::write_str("\n");

            if command == b"tasks" {
                saw_tasks = true;
            } else if command == b"fs ls /" {
                saw_ls = true;
                fs_reads += 1;
            } else if command == b"fs cat /INDEX.HTM" {
                saw_cat_index = true;
                fs_reads += 1;
            } else if command == b"fs cat /CHAIN.HTM" {
                fs_reads += 1;
            }
            self.dispatch_cli_command(command)?;
            if command == b"queues" && !operator_done {
                serial::write_str("DPMK:CLI-OPERATOR-OK\n");
                operator_done = true;
            }
            serial::write_str("DPMK:FAIR-CLI-END:");
            serial::write_decimal(round);
            serial::write_str(":OK\n");
            cli_served += 1;
            work_units += 1;

            serial::write_str("DPMK:FAIR-ROUND:");
            serial::write_decimal(round);
            serial::write_str(" http=");
            serial::write_decimal(http_served);
            serial::write_str(" cli=");
            serial::write_decimal(cli_served);
            serial::write_str(" fs=");
            serial::write_decimal(fs_reads);
            serial::write_str(" timer=");
            serial::write_decimal(timer_ticks);
            serial::write_str(" net=");
            serial::write_decimal(net_polls);
            serial::write_str("\n");
        }

        let gap = work_units - last_timer_unit;
        if gap > max_gap {
            max_gap = gap;
        }

        if http_served != STAGE_G_ROUNDS
            || cli_served != STAGE_G_ROUNDS
            || timer_ticks < STAGE_G_ROUNDS
            || fs_reads < STAGE_G_ROUNDS + 2
            || !saw_tasks
            || !saw_ls
            || !saw_cat_index
        {
            return Err("fair-progress");
        }

        serial::write_str("DPMK:FAIR-HTTP-OK:");
        serial::write_decimal(http_served);
        serial::write_str("\n");
        serial::write_str("DPMK:FAIR-CLI-OK:");
        serial::write_decimal(cli_served);
        serial::write_str("\n");
        serial::write_str("DPMK:FAIR-FS-OK:");
        serial::write_decimal(fs_reads);
        serial::write_str("\n");
        serial::write_str("DPMK:FAIR-TIMER-OK:");
        serial::write_decimal(timer_ticks);
        serial::write_str("\n");
        serial::write_str("DPMK:FAIR-NET:");
        serial::write_decimal(net_polls);
        serial::write_str("\n");
        serial::write_str("DPMK:FAIR-TIMER-MAXGAP:");
        serial::write_decimal(max_gap);
        serial::write_str("\n");

        if !prove_fault_containment() {
            return Err("fair-fault");
        }
        serial::write_str("DPMK:FAIR-FAULT-CONTAINED\n");
        serial::write_str("DPMK:FAIR-OK\n");
        Ok(())
    }

    fn fairness_poll_network(
        &mut self,
        http_files: &HttpFileSet<'_>,
    ) -> Result<Option<NetworkReplyKind>, &'static str> {
        if !self.net_ready {
            return Ok(None);
        }
        let Some(net_reply) = self.net_task.receive_raw_frame()? else {
            return Ok(None);
        };
        send_to_task(
            &mut self.tasks,
            TASK_TCPIP,
            &mut self.tcpip_mailbox,
            net_reply,
        )?;
        let tcpip_received = recv_from_task(&mut self.tasks, TASK_TCPIP, &mut self.tcpip_mailbox)?;
        let response = protected_net_region(|memory| {
            self.tcpip_task.handle_net_frame(
                tcpip_received,
                memory.frame_from_net_message(tcpip_received)?,
                &mut self.http_task,
                http_files,
            )
        })?;
        if let Some(response) = response {
            let kind = response.kind;
            self.net_task.transmit_reply(&response)?;
            return Ok(Some(kind));
        }
        Ok(None)
    }
}
