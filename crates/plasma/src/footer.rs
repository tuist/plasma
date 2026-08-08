//! A status footer that shows the worktree, the associated pull request, and
//! the CI checks for it.
//!
//! The data is collected in a background thread so the TUI frame loop never
//! blocks on `git` or `gh` calls. The thread updates a shared `FooterState`
//! every few seconds; the UI snapshots it at draw time.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(4);

/// One chunk of data the footer renders. Kept in a single struct so the
/// background thread can fill all fields in one critical section.
#[derive(Clone, Default)]
struct FooterState {
    branch: Option<String>,
    pr: Option<PrInfo>,
    ci: Option<CiStatus>,
    /// When the last successful refresh happened. `None` until the first
    /// poll completes; the UI uses it to show a stale indicator.
    refreshed_at: Option<Instant>,
}

#[derive(Clone, Debug)]
struct PrInfo {
    number: u64,
    title: String,
    url: String,
    state: PrState,
    is_draft: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrState {
    Open,
    Merged,
    Closed,
}

impl PrState {
    fn parse(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "MERGED" => Self::Merged,
            "CLOSED" => Self::Closed,
            _ => Self::Open,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct CiStatus {
    passing: u32,
    failing: u32,
    pending: u32,
}

pub struct Footer {
    working_dir: PathBuf,
    state: Arc<Mutex<FooterState>>,
    _refresh: thread::JoinHandle<()>,
}

impl Footer {
    /// Build a footer for the given working directory. The background
    /// refresh thread is started immediately.
    pub fn new(working_dir: PathBuf) -> Self {
        let state = Arc::new(Mutex::new(FooterState::default()));
        let refresh_state = Arc::clone(&state);
        let refresh_dir = working_dir.clone();
        let refresh = thread::spawn(move || {
            refresh_loop(refresh_dir, refresh_state);
        });
        Self {
            working_dir,
            state,
            _refresh: refresh,
        }
    }

    /// Render the footer as a stack of `Line`s. The caller decides how
    /// many rows to actually display.
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let state = self.state.lock().expect("footer mutex poisoned").clone();
        let mut lines = Vec::new();

        // Line 1: worktree path + branch, with PR info on the right.
        let left = format!(
            "{} ({})",
            shorten_path(&self.working_dir),
            state.branch.as_deref().unwrap_or("detached"),
        );
        let pr_summary = state
            .pr
            .as_ref()
            .map(format_pr)
            .unwrap_or_else(|| "no PR associated".to_string());
        lines.push(line_with_right(width, &left, &pr_summary, pr_style(&state.pr)));

        // Line 2: CI status, with model info on the right.
        let ci_summary = match state.ci {
            Some(ci) => format_ci(ci),
            None => "CI: \u{2014}".to_string(),
        };
        let right = "(openrouter) model \u{b7} effort".to_string();
        lines.push(line_with_right(width, &ci_summary, &right, Style::default()));

        lines
    }
}

// -- Rendering helpers --------------------------------------------------------

fn line_with_right(
    width: u16,
    left: &str,
    right: &str,
    right_style: Style,
) -> Line<'static> {
    let left = left.to_string();
    let right = right.to_string();
    let available = width as usize;
    if available == 0 {
        return Line::from(Span::raw(left));
    }
    if left.len() + right.len() + 1 >= available {
        return Line::from(Span::raw(truncate(&left, available)));
    }
    let padding = available - left.len() - right.len();
    let mut spans = vec![Span::raw(left), Span::raw(" ".repeat(padding))];
    spans.push(Span::styled(right, right_style));
    Line::from(spans)
}

fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        return input.to_string();
    }
    if max <= 1 {
        return "\u{2026}".to_string();
    }
    let mut out = input[..max - 1].to_string();
    out.push('\u{2026}');
    out
}

fn shorten_path(path: &Path) -> String {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let stripped = home
        .as_path()
        .ancestors()
        .find(|ancestor| path.strip_prefix(ancestor).is_ok())
        .and_then(|ancestor| path.strip_prefix(ancestor).ok())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| path.to_path_buf());
    let mut display = if home.as_os_str().is_empty() {
        stripped.display().to_string()
    } else {
        format!("~{}", stripped.display())
    };
    // Keep at most the last two components so a long path does not eat
    // the footer.
    let components: Vec<&str> = display.split('/').filter(|c| !c.is_empty()).collect();
    if components.len() > 3 {
        display = format!(
            "\u{2026}/{}",
            components[components.len() - 2..].join("/")
        );
    }
    display
}

fn format_pr(pr: &PrInfo) -> String {
    let prefix = if pr.is_draft { "Draft " } else { "" };
    let title = one_line(&pr.title, 60);
    format!(
        "{prefix}PR #{n} \u{2014} {title} [{state}]",
        prefix = prefix,
        n = pr.number,
        title = title,
        state = pr.state.label(),
    )
}

fn format_ci(ci: CiStatus) -> String {
    let mut parts = Vec::new();
    if ci.passing > 0 {
        parts.push(format!("\u{2713} {pass} passing", pass = ci.passing));
    }
    if ci.failing > 0 {
        parts.push(format!("\u{2717} {fail} failing", fail = ci.failing));
    }
    if ci.pending > 0 {
        parts.push(format!("\u{22ef} {pending} pending", pending = ci.pending));
    }
    if parts.is_empty() {
        "CI: no checks".to_string()
    } else {
        format!("CI: {}", parts.join(", "))
    }
}

fn pr_style(pr: &Option<PrInfo>) -> Style {
    match pr {
        Some(pr) if pr.state == PrState::Open && !pr.is_draft => {
            Style::default().fg(Color::Cyan)
        }
        Some(pr) if pr.state == PrState::Merged => Style::default().fg(Color::Green),
        Some(pr) if pr.state == PrState::Closed => Style::default().fg(Color::Red),
        Some(_) => Style::default().fg(Color::DarkGray),
        None => Style::default().fg(Color::DarkGray),
    }
}

fn one_line(input: &str, max: usize) -> String {
    let cleaned = input.replace('\n', " ").replace('\r', "");
    truncate(&cleaned, max)
}

// -- Background refresh -------------------------------------------------------

fn refresh_loop(working_dir: PathBuf, state: Arc<Mutex<FooterState>>) {
    loop {
        // Errors are intentionally swallowed: a failing `gh` invocation
        // should not write to stderr while the TUI is drawing, and the
        // footer should simply stop showing the data that failed to
        // refresh until the next successful poll.
        refresh_once(&working_dir, &state);
        thread::sleep(REFRESH_INTERVAL);
    }
}

fn refresh_once(working_dir: &Path, state: &Arc<Mutex<FooterState>>) {
    let branch = git_branch(working_dir);
    // PR info and CI status are queried independently: a missing
    // `gh pr checks` result (e.g. before CI has run) should not blank
    // out the PR we just found.
    let pr = branch
        .as_deref()
        .and_then(|branch| gh_pr_view(working_dir, branch).ok().flatten());
    let ci = pr
        .as_ref()
        .and_then(|info| gh_pr_checks(working_dir, info.number).ok().flatten());
    let mut guard = state.lock().expect("footer mutex poisoned");
    guard.branch = branch;
    guard.pr = pr;
    guard.ci = ci;
    guard.refreshed_at = Some(Instant::now());
}

fn git_branch(working_dir: &Path) -> Option<String> {
    run_command(
        "git",
        &["rev-parse", "--abbrev-ref", "HEAD"],
        working_dir,
    )
    .ok()
    .filter(|output| !output.is_empty())
}

fn gh_pr_view(working_dir: &Path, branch: &str) -> anyhow::Result<Option<PrInfo>> {
    let Some(output) = run_command(
        "gh",
        &[
            "pr",
            "view",
            branch,
            "--json",
            "number,title,url,state,isDraft",
        ],
        working_dir,
    )
    .ok()
    else {
        return Ok(None);
    };
    if output.is_empty() {
        return Ok(None);
    }
    let parsed: serde_json::Value = serde_json::from_str(&output)?;
    let number = parsed
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing PR number"))?;
    let title = parsed
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let url = parsed
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();
    let state = parsed
        .get("state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("OPEN")
        .to_string();
    let is_draft = parsed
        .get("isDraft")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Ok(Some(PrInfo {
        number,
        title,
        url,
        state: PrState::parse(&state),
        is_draft,
    }))
}

fn gh_pr_checks(working_dir: &Path, number: u64) -> anyhow::Result<Option<CiStatus>> {
    let output = run_command(
        "gh",
        &[
            "pr",
            "checks",
            &number.to_string(),
            "--json",
            "name,conclusion",
        ],
        working_dir,
    )?;
    if output.is_empty() {
        return Ok(None);
    }
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&output)?;
    let mut passing = 0u32;
    let mut failing = 0u32;
    let mut pending = 0u32;
    for check in parsed {
        match check
            .get("conclusion")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
        {
            "SUCCESS" => passing += 1,
            "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" => failing += 1,
            _ => pending += 1,
        }
    }
    Ok(Some(CiStatus {
        passing,
        failing,
        pending,
    }))
}

fn run_command(program: &str, args: &[&str], working_dir: &Path) -> anyhow::Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(working_dir)
        .env_remove("GH_TOKEN")
        .env("NO_COLOR", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let start = Instant::now();
    loop {
        if start.elapsed() > COMMAND_TIMEOUT {
            let _ = child.kill();
            anyhow::bail!("{program} {} timed out", args.join(" "));
        }
        match child.try_wait()? {
            Some(status) => {
                if !status.success() {
                    anyhow::bail!(
                        "{program} {} exited with {status}",
                        args.join(" ")
                    );
                }
                let mut output = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    use std::io::Read;
                    stdout.read_to_string(&mut output)?;
                }
                return Ok(output.trim().to_string());
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_path_replaces_home_with_tilde() {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        if home.as_os_str().is_empty() {
            return;
        }
        let path = home.join("work").join("plasma");
        let shortened = shorten_path(&path);
        assert!(shortened.starts_with('~'), "got {shortened}");
        assert!(shortened.ends_with("work/plasma"), "got {shortened}");
    }

    #[test]
    fn shorten_path_keeps_last_two_components_for_long_paths() {
        let path = PathBuf::from("/a/very/long/path/to/the/worktree");
        let shortened = shorten_path(&path);
        assert!(shortened.starts_with('\u{2026}'), "got {shortened}");
        assert!(shortened.ends_with("the/worktree"), "got {shortened}");
    }

    #[test]
    fn truncate_respects_max_length() {
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
        assert_eq!(truncate("hi", 5), "hi");
    }

    #[test]
    fn format_ci_aggregates_counts() {
        let ci = CiStatus {
            passing: 3,
            failing: 1,
            pending: 2,
        };
        let rendered = format_ci(ci);
        assert!(rendered.contains("3 passing"));
        assert!(rendered.contains("1 failing"));
        assert!(rendered.contains("2 pending"));
    }

    #[test]
    fn format_ci_handles_empty() {
        let ci = CiStatus {
            passing: 0,
            failing: 0,
            pending: 0,
        };
        assert_eq!(format_ci(ci), "CI: no checks");
    }

    #[test]
    fn pr_state_parses_known_values() {
        assert_eq!(PrState::parse("OPEN"), PrState::Open);
        assert_eq!(PrState::parse("merged"), PrState::Merged);
        assert_eq!(PrState::parse("CLOSED"), PrState::Closed);
        assert_eq!(PrState::parse("unknown"), PrState::Open);
    }
}
