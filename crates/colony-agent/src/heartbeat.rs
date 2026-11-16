//! Runs a heartbeat tick through `colony-agent`.
//!
//! `colony-exec` owns the heartbeat scheduler but cannot depend on `colony-agent`, so each
//! binary that runs heartbeats (the shell, the `colony-events` daemon) installs this as the
//! scheduler's agent callback at startup ([`install`]).
//!
//! A run gets a fresh agent with the heartbeat system prompt and only the tools its
//! [`SecurityMode`] allows that need no approval: a heartbeat runs with nobody at the
//! terminal, so a tool that would ask is not offered. Those tools (`file_read`, `search`)
//! only reach paths inside the checklist's directory. It honours the checklist's
//! `timeout`, `max_tool_calls` and `token_budget`, and turns the model's output into
//! findings with the scheduler's own extractor.
//!
//! A named agent with an image runs in its own container instead (`config.container`,
//! set by the scheduler). Its one tool, `shell`, runs a command there through Colony's
//! shell, in `/work`, a scratch copy of the agent's workdir; the container reaches only the
//! hosts the agent was granted.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::tool::{builtins, Tool, ToolExecutionError, ToolOutput, ToolParam};
use crate::{Agent, AgentConfig, BudgetWarning, TokenBudget, ToolClassification, ToolRegistry};
use async_trait::async_trait;
use colony_exec::heartbeat::{
    build_checklist_prompt, build_system_prompt, derive_status, extract_findings,
    set_default_agent_callback, HeartbeatConfig, HeartbeatError, HeartbeatRun, HeartbeatStatus,
    SecurityMode, ToolCall,
};

/// The message that sends the model back to work after its tools have answered.
const CONTINUE: &str = "Continue the checklist using the tool results above. \
                        Finish with one PASS, WARN or ALERT line per item.";

/// Install [`run`] as the agent callback for every heartbeat scheduler in this process.
pub fn install() {
    set_default_agent_callback(Arc::new(run));
}

/// Run one heartbeat tick. Blocks until the run ends; called by the scheduler from a
/// blocking context. The agent runs on its own thread and runtime, so this is safe to
/// call whether or not the caller is inside a tokio runtime.
fn run(
    config: &HeartbeatConfig,
    path: &Path,
    checklist: &str,
) -> Result<HeartbeatRun, HeartbeatError> {
    let config = config.clone();
    let path = path.to_path_buf();
    let checklist = checklist.to_string();
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_agent(&config, &path, &checklist))
    })
    .join()
    .map_err(|_| HeartbeatError::Agent("heartbeat agent thread panicked".to_string()))?
}

async fn run_agent(
    config: &HeartbeatConfig,
    path: &Path,
    checklist: &str,
) -> Result<HeartbeatRun, HeartbeatError> {
    let started = Instant::now();
    let mut agent = Agent::with_config(agent_config(config))
        .await
        .map_err(|e| HeartbeatError::NoProvider(e.to_string()))?;
    let root = config
        .workdir
        .as_deref()
        .unwrap_or_else(|| path.parent().unwrap_or(path))
        .canonicalize()
        .map_err(HeartbeatError::Io)?;
    let cwd = if let Some(container) = &config.container {
        *agent.tools_mut() = container_tools(container);
        "/work".to_string()
    } else {
        *agent.tools_mut() = unattended_tools(config.mode, &root);
        root.display().to_string()
    };

    let prompt = build_checklist_prompt(checklist, None, &cwd, None);
    let limit = Duration::from_secs(config.timeout_secs);

    // Set from outside (a watch on the run's container) to stop the run now.
    let cancelled = async {
        let Some(flag) = config.cancel.as_ref() else {
            return std::future::pending().await;
        };
        while !flag.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    let finished = tokio::select! {
        r = tokio::time::timeout(limit, turns(&mut agent, prompt)) => Some(r),
        () = cancelled => None,
    };
    let (status, raw_output) = match finished {
        None => (
            HeartbeatStatus::Error,
            "stopped: the run was cancelled".to_string(),
        ),
        Some(Err(_)) => (
            HeartbeatStatus::Timeout,
            format!("stopped: no answer within {}s", config.timeout_secs),
        ),
        Some(Ok(Err(e))) => (HeartbeatStatus::Error, e.to_string()),
        Some(Ok(Ok(output))) if agent.budget_warning() == BudgetWarning::Exceeded => (
            HeartbeatStatus::Error,
            format!(
                "{output}\n[stopped: token budget of {} exceeded]",
                config.token_budget
            ),
        ),
        Some(Ok(Ok(output))) => (derive_status(&extract_findings(&output)), output),
    };
    let findings = match status {
        HeartbeatStatus::Timeout | HeartbeatStatus::Error => Vec::new(),
        _ => extract_findings(&raw_output),
    };

    Ok(HeartbeatRun {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp(),
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        status,
        findings,
        raw_output,
        tokens_used: agent.usage_stats().total_input_tokens
            + agent.usage_stats().total_output_tokens,
        tool_calls: agent
            .tool_log()
            .iter()
            .map(|t| ToolCall {
                tool: t.name.clone(),
                input: t.input.clone(),
                output: t.output.clone(),
            })
            .collect(),
        // The scheduler fills these in.
        checklist_path: String::new(),
        interval_secs: 0,
    })
}

/// Turn the model until it stops calling tools, runs out of tool calls or budget.
/// Returns everything the model wrote.
async fn turns(agent: &mut Agent, prompt: String) -> crate::Result<String> {
    let max_calls = agent.config().max_tool_calls.unwrap_or(u32::MAX);
    let mut input = prompt;
    let mut output = String::new();
    loop {
        let before = agent.tool_call_count();
        let text = agent.process_full(input).await?;
        if !text.is_empty() {
            // Turns are joined by a line break, with none added at the end: the run's
            // output is recorded as evidence and must be exactly what the model wrote.
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&text);
        }
        let called_tools = agent.tool_call_count() > before;
        if !called_tools
            || agent.tool_call_count() >= max_calls
            || agent.budget_warning() == BudgetWarning::Exceeded
        {
            return Ok(output);
        }
        input = CONTINUE.to_string();
    }
}

fn agent_config(config: &HeartbeatConfig) -> AgentConfig {
    AgentConfig {
        system_prompt: build_system_prompt(),
        token_budget: Some(TokenBudget::new(u64::from(config.token_budget))),
        max_tool_calls: Some(config.max_tool_calls),
        echo_stream: false,
        ..AgentConfig::default()
    }
}

/// The tools an unattended run in `mode` gets: those `mode` allows that need no approval,
/// each confined to `root`.
// ponytail: approval tools are dropped, not routed to a person; asks that park a run come in step 6
fn unattended_tools(mode: SecurityMode, root: &Path) -> ToolRegistry {
    let allowed = mode.allowed_tools();
    let mut tools = ToolRegistry::new();
    let candidates: [Box<dyn Tool>; 2] = [Box::new(builtins::FileRead), Box::new(builtins::Search)];
    for inner in candidates {
        if allowed.contains(&inner.name()) {
            let tool = Confined {
                inner,
                root: root.to_path_buf(),
            };
            tools.register_tool(Box::new(tool), ToolClassification::Programmatic);
        }
    }
    tools
}

/// The tools a run in a container gets: `shell`, there.
fn container_tools(container: &str) -> ToolRegistry {
    let mut tools = ToolRegistry::new();
    tools.register_tool(
        Box::new(ContainerShell {
            container: container.to_string(),
        }),
        ToolClassification::Programmatic,
    );
    tools
}

/// Longest a `shell` command may run.
const SHELL_TIMEOUT: Duration = Duration::from_secs(120);
/// Most bytes of a command's stdout, and of its stderr, the model gets.
const SHELL_OUTPUT_MAX: u64 = 16 * 1024;

/// Runs a command in the run's container through Colony's shell (`colony -c`).
struct ContainerShell {
    container: String,
}

/// Read at most `SHELL_OUTPUT_MAX` bytes. `true` if there was more.
async fn capped(pipe: Option<impl tokio::io::AsyncRead + Unpin>) -> (String, bool) {
    use tokio::io::AsyncReadExt;
    let Some(pipe) = pipe else {
        return (String::new(), false);
    };
    let mut buf = Vec::new();
    let _ = pipe.take(SHELL_OUTPUT_MAX + 1).read_to_end(&mut buf).await;
    let more = buf.len() as u64 > SHELL_OUTPUT_MAX;
    buf.truncate(usize::try_from(SHELL_OUTPUT_MAX).unwrap_or(usize::MAX));
    (String::from_utf8_lossy(&buf).to_string(), more)
}

#[async_trait]
impl Tool for ContainerShell {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run a command in this run's container through Colony's shell (bash-compatible), in \
         /work: a scratch copy of the project. What you change there is the run's diff and \
         never touches the real project. Only the hosts this agent was granted can be reached; \
         a call to any other is refused."
    }

    fn parameters(&self) -> Vec<ToolParam> {
        vec![ToolParam {
            name: "command".to_string(),
            param_type: crate::tool::ParamType::String,
            required: true,
            description: "The command line to run".to_string(),
        }]
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolExecutionError> {
        let Some(command) = params.get("command").and_then(serde_json::Value::as_str) else {
            return Ok(ToolOutput::failure("missing command"));
        };
        let mut cmd = tokio::process::Command::from(colony_exec::sandbox::podman());
        let spawned = cmd
            .args(["exec", &self.container, "colony", "-c", command])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(e) => return Ok(ToolOutput::failure(format!("podman: {e}"))),
        };
        let run = async {
            let ((stdout, more_out), (stderr, more_err)) =
                tokio::join!(capped(child.stdout.take()), capped(child.stderr.take()));
            (stdout, stderr, more_out || more_err)
        };
        let Ok((stdout, stderr, truncated)) = tokio::time::timeout(SHELL_TIMEOUT, run).await else {
            let _ = child.kill().await;
            return Ok(ToolOutput::failure(format!(
                "stopped: the command ran past {}s",
                SHELL_TIMEOUT.as_secs()
            )));
        };
        if truncated {
            // The rest is not read, so the command may be blocked writing it.
            let _ = child.kill().await;
        }
        let code = child.wait().await.ok().and_then(|s| s.code());
        Ok(ToolOutput::success(serde_json::json!({
            "exit": code,
            "stdout": stdout,
            "stderr": stderr,
            "truncated": truncated,
        })))
    }
}

/// A tool whose `path` argument must resolve inside `root` (canonical). A relative path is
/// taken from `root`; a missing one means `root`. Symlinks are resolved before the check.
// ponytail: secrets inside the project (.env) stay readable; add a deny list if checklists need it
struct Confined {
    inner: Box<dyn Tool>,
    root: PathBuf,
}

impl Confined {
    fn resolve(&self, path: Option<&str>) -> Result<PathBuf, String> {
        let path = self.root.join(path.unwrap_or("."));
        let real = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if real.starts_with(&self.root) {
            Ok(real)
        } else {
            Err(format!(
                "{} is outside {}, the only directory this run may read",
                path.display(),
                self.root.display()
            ))
        }
    }
}

#[async_trait]
impl Tool for Confined {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> Vec<ToolParam> {
        self.inner.parameters()
    }

    async fn execute(
        &self,
        mut params: serde_json::Value,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let path = params.get("path").and_then(serde_json::Value::as_str);
        match self.resolve(path) {
            Ok(real) => {
                params["path"] = serde_json::Value::String(real.display().to_string());
                self.inner.execute(params).await
            }
            Err(why) => Ok(ToolOutput::failure(why)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unattended_runs_are_offered_no_tool_that_asks_for_approval() {
        // shell_exec and file_write ask for approval, and nobody is there to give it.
        for mode in [
            SecurityMode::Notify,
            SecurityMode::Active,
            SecurityMode::Full,
        ] {
            let tools = unattended_tools(mode, Path::new("/"));
            let mut names = tools.tool_names();
            names.sort_unstable();
            assert_eq!(names, ["file_read", "search"], "{mode}");
        }
    }

    #[tokio::test]
    async fn unattended_tools_only_reach_the_checklist_directory() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("project");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("in.txt"), "inside").unwrap();
        std::fs::write(outer.path().join("out.txt"), "outside").unwrap();
        std::os::unix::fs::symlink(outer.path().join("out.txt"), root.join("link.txt")).unwrap();
        let root = root.canonicalize().unwrap();
        let tools = unattended_tools(SecurityMode::Notify, &root);
        let read = |path: &str| tools.execute("file_read", serde_json::json!({ "path": path }));

        let ok = read("in.txt").await.unwrap();
        assert!(
            ok.success && ok.data.to_string().contains("inside"),
            "{ok:?}"
        );
        let ok = read(root.join("in.txt").to_str().unwrap()).await.unwrap();
        assert!(ok.success);

        let outside = outer.path().join("out.txt");
        for path in [
            "../out.txt",
            "link.txt",
            outside.to_str().unwrap(),
            "/etc/hosts",
        ] {
            let refused = read(path).await.unwrap();
            assert!(!refused.success, "{path} was read");
            assert!(!refused.data.to_string().contains("outside"), "{path}");
        }

        let found = tools
            .execute("search", serde_json::json!({ "pattern": "inside" }))
            .await
            .unwrap();
        assert_eq!(found.data["count"], 1, "{found:?}");
        let refused = tools
            .execute(
                "search",
                serde_json::json!({ "pattern": "outside", "path": ".." }),
            )
            .await
            .unwrap();
        assert!(!refused.success);
    }

    #[test]
    fn the_agent_gets_the_checklist_limits() {
        let config = HeartbeatConfig {
            max_tool_calls: 7,
            token_budget: 1234,
            ..HeartbeatConfig::default()
        };
        let agent = agent_config(&config);
        assert_eq!(agent.max_tool_calls, Some(7));
        assert_eq!(agent.token_budget.map(|b| b.max_tokens), Some(1234));
        assert!(!agent.echo_stream);
        assert!(agent.system_prompt.contains("heartbeat monitor"));
    }
}
