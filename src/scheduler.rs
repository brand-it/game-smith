use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use loco_rs::app::AppContext;
use loco_rs::task::{Task as LocoTask, Vars};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Config (mirrors config/scheduler.yaml)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SchedulerConfig {
    output: Option<OutputMode>,
    jobs: HashMap<String, JobConfig>,
}

#[derive(Debug, Deserialize)]
struct JobConfig {
    run: String,
    #[serde(default)]
    schedule: Option<String>,
    #[serde(default)]
    output: Option<OutputMode>,
    #[serde(default = "default_run_on_start")]
    run_on_start: bool,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum OutputMode {
    Stdout,
    Silent,
}

const fn default_run_on_start() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Task registry — maps job names to Arc<task> for safe sharing across tasks
// ---------------------------------------------------------------------------

struct TaskRegistry {
    tasks: HashMap<String, Arc<dyn LocoTask>>,
}

impl TaskRegistry {
    fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    fn register(&mut self, task: impl LocoTask + 'static) {
        let name = task.task().name;
        tracing::info!(task = %name, "registering scheduled task");
        self.tasks.insert(name, Arc::new(task));
    }

    fn get(&self, name: &str) -> Option<Arc<dyn LocoTask>> {
        self.tasks.get(name).cloned()
    }
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// Start the in-process scheduler.
///
/// Reads `config/scheduler.yaml` from the current working directory, registers
/// the provided tasks, and spawns a tokio task per job. Each task runs its
/// `Task::run(&ctx, &vars)` method directly — no subprocess spawning.
///
/// Returns `Ok(())` even when the config file is missing (treated as "no jobs").
///
/// # Errors
///
/// Returns an error only if the scheduler handles fail unexpectedly. Individual
/// job failures are logged but do not propagate as errors.
pub async fn run_scheduler(ctx: &AppContext) -> loco_rs::Result<()> {
    let config_path = PathBuf::from("config/scheduler.yaml");

    let config = match std::fs::read_to_string(&config_path) {
        Ok(contents) => match serde_yaml::from_str::<SchedulerConfig>(&contents) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    path = %config_path.display(),
                    error = %e,
                    "failed to parse scheduler config, scheduler disabled"
                );
                return Ok(());
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(
                path = %config_path.display(),
                "scheduler config not found, scheduler disabled"
            );
            return Ok(());
        }
        Err(e) => {
            tracing::error!(
                path = %config_path.display(),
                error = %e,
                "failed to read scheduler config, scheduler disabled"
            );
            return Ok(());
        }
    };

    let global_output = config.output.unwrap_or(OutputMode::Stdout);

    let mut registry = TaskRegistry::new();
    registry.register(crate::tasks::pid_liveness::PidLiveness);

    let mut handles = Vec::new();

    for (job_name, job_cfg) in config.jobs {
        let task_name = &job_cfg.run;

        let Some(task) = registry.get(task_name) else {
            tracing::warn!(
                job = %job_name,
                task = %task_name,
                "scheduled job references unknown task, skipping"
            );
            continue;
        };

        let output = job_cfg.output.unwrap_or(global_output);
        let schedule = job_cfg.schedule.clone();
        let tags = job_cfg.tags;

        tracing::info!(
            job = %job_name,
            task = %task_name,
            schedule = %schedule.as_deref().unwrap_or("none"),
            run_on_start = job_cfg.run_on_start,
            output = ?output,
            tags = ?tags,
            "scheduling job"
        );

        handles.push(tokio::spawn(run_job(
            job_name,
            task,
            schedule,
            output,
            job_cfg.run_on_start,
            ctx.clone(),
        )));
    }

    if handles.is_empty() {
        tracing::info!("no scheduled jobs configured");
        return Ok(());
    }

    tracing::info!(count = handles.len(), "scheduler started");

    // Await all jobs in the background (errors logged inside each job task).
    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!(error = %e, "scheduler job task panicked");
        }
    }

    Ok(())
}

/// Run a single scheduled job: optional immediate run, then recurring interval.
async fn run_job(
    job_name: String,
    task: Arc<dyn LocoTask>,
    schedule: Option<String>,
    output: OutputMode,
    run_on_start: bool,
    ctx: AppContext,
) {
    // Run immediately on start if configured.
    if run_on_start {
        let _ = execute_task(&job_name, &*task, &ctx, output).await;
    }

    // Parse schedule and enter the interval loop.
    let Some(schedule) = schedule else {
        tracing::info!(job = %job_name, "no schedule configured, job will not repeat");
        return;
    };

    let interval = match parse_interval(&schedule) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(job = %job_name, error = %e, "failed to parse schedule, job will not repeat");
            return;
        }
    };

    let mut ticker = tokio::time::interval(interval);
    // Skip the first tick (fires immediately) — we already ran on start if requested.
    ticker.tick().await;

    loop {
        ticker.tick().await;
        let _ = execute_task(&job_name, &*task, &ctx, output).await;
    }
}

/// Execute a task and handle output / error logging.
async fn execute_task(
    job_name: &str,
    task: &dyn LocoTask,
    ctx: &AppContext,
    output: OutputMode,
) -> loco_rs::Result<()> {
    let vars = Vars::default();

    match task.run(ctx, &vars).await {
        Ok(()) => {
            if matches!(output, OutputMode::Stdout) {
                tracing::info!(job = %job_name, "task completed");
            }
            Ok(())
        }
        Err(e) => {
            tracing::error!(job = %job_name, error = %e, "task failed");
            Err(e)
        }
    }
}

/// Parse a schedule string into a tokio duration.
///
/// Supports:
/// - "every N seconds" / "every N minutes" / "every N hours"
/// - Cron expressions (e.g. "0 * * * *")
fn parse_interval(schedule: &str) -> std::result::Result<tokio::time::Duration, String> {
    // Handle "every N <unit>" format.
    if let Some(rest) = schedule.strip_prefix("every ") {
        return parse_every(rest);
    }

    // Handle cron expressions via the `cron` crate.
    parse_cron_interval(schedule)
}

fn parse_every(input: &str) -> std::result::Result<tokio::time::Duration, String> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(format!("invalid 'every' schedule: {input}"));
    }

    let count: u64 = parts[0]
        .parse()
        .map_err(|_e| format!("invalid count '{}' in schedule: {input}", parts[0]))?;

    if count == 0 {
        return Err(format!("count must be > 0 in schedule: {input}"));
    }

    let unit = parts[1];
    let secs = match unit {
        "second" | "seconds" => count,
        "minute" | "minutes" => count * 60,
        "hour" | "hours" => count * 3600,
        _ => return Err(format!("unsupported unit '{unit}' in schedule: {input}")),
    };

    Ok(tokio::time::Duration::from_secs(secs))
}

fn parse_cron_interval(schedule: &str) -> std::result::Result<tokio::time::Duration, String> {
    use cron::Schedule;
    use std::str::FromStr;

    let schedule = Schedule::from_str(schedule)
        .map_err(|e| format!("invalid cron expression '{schedule}': {e}"))?;

    let now = chrono::Utc::now();
    let mut upcoming = schedule.upcoming(chrono::Utc);

    match upcoming.next() {
        Some(first) => {
            let duration = first
                .signed_duration_since(now)
                .to_std()
                .map_err(|e| format!("duration conversion error: {e}"))?;
            Ok(duration)
        }
        None => Err(format!("no upcoming times for cron expression: {schedule}")),
    }
}
