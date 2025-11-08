use crate::observability::metrics::get_metrics;
use anyhow::Result;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tokio::time::sleep;

pub async fn collect_process_metrics(is_metrics_enabled: bool) -> Result<()> {
    if !is_metrics_enabled {
        return Ok(());
    }
    let metrics = get_metrics().await;
    let mut sys = System::new_all();
    let pid = sysinfo::get_current_pid().unwrap();

    let start_time_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    metrics.process_start_time.set(start_time_unix);

    loop {
        // Refresh just our process
        let pids = [pid];
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            false,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_tasks()
                .with_exe(UpdateKind::OnlyIfNotSet),
        );

        if let Some(proc) = sys.process(pid) {
            let cpu = proc.cpu_usage(); // Now this gives a % over last interval
            metrics.process_cpu_usage.set(cpu.into());

            let mem = proc.memory() as i64;
            metrics.process_memory_usage.set(mem);

            #[cfg(target_family = "unix")]
            {
                use std::fs;
                if let Ok(entries) = fs::read_dir(format!("/proc/{}/fd", pid.as_u32())) {
                    metrics.process_open_fds.set(entries.count() as i64);
                }
            }

            let uptime = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64 - start_time_unix;
            metrics.process_uptime.set(uptime);
        }

        sleep(Duration::from_secs(5)).await;
    }
}
