use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use plasma_inference::ToolDefinition;
use serde_json::{Value, json};

/// Outcome of running a tool. `Ok` carries the textual output the model
/// sees; `Err` carries an error message that the model is told about.
pub type ToolResult = Result<String, String>;

/// A tool the model can call. The host owns a `Vec<Box<dyn Tool>>` and
/// hands it to the session, which dispatches incoming `ToolCall`s by name.
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// JSON Schema describing the function's parameters.
    fn parameters(&self) -> Value;
    fn execute(&self, arguments: &Value) -> ToolResult;

    /// Convenience helper that returns the OpenAI-compatible tool
    /// definition the provider expects.
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }
}

pub fn workspace_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in root.read_dir()? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

/// `read(path)` — return the contents of a file as text. The path is
/// resolved relative to `root` when it is not absolute.
pub struct ReadTool {
    pub root: PathBuf,
}

impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a file at the given path. Paths are resolved relative to the workspace root unless absolute."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read, relative to the workspace root or absolute."
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, arguments: &Value) -> ToolResult {
        let raw = arguments
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing 'path' argument".to_string())?;
        let path = resolve(&self.root, raw);
        let mut file = std::fs::File::open(&path)
            .map_err(|error| format!("could not open {}: {error}", path.display()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        if contents.len() > 50_000 {
            contents.truncate(50_000);
            contents.push_str("\n... (truncated)");
        }
        Ok(contents)
    }
}

/// `bash(command)` — run a shell command and return its combined
/// `stdout` + `stderr`. The command runs in `root` with a hard timeout.
pub struct BashTool {
    pub root: PathBuf,
    pub timeout: Duration,
}

impl Default for BashTool {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            timeout: Duration::from_secs(30),
        }
    }
}

impl Tool for BashTool {
    fn name(&self) -> &'static str {
        "bash"
    }

    fn description(&self) -> &'static str {
        "Run a shell command in the workspace root and return its combined stdout and stderr. The command has a hard timeout."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute. Runs with sh -c."
                }
            },
            "required": ["command"]
        })
    }

    fn execute(&self, arguments: &Value) -> ToolResult {
        let command = arguments
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing 'command' argument".to_string())?;
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.root)
            .env_remove("GH_TOKEN")
            .env("NO_COLOR", "1")
            .output()
            .map_err(|error| format!("could not spawn shell: {error}"))?;
        let mut combined = String::new();
        if !output.stdout.is_empty() {
            combined.push_str(&String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        if combined.len() > 50_000 {
            combined.truncate(50_000);
            combined.push_str("\n... (truncated)");
        }
        if !output.status.success() {
            return Err(format!(
                "command exited with status {}: {combined}",
                output.status
            ));
        }
        Ok(combined)
    }
}

fn resolve(root: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "plasma-tools-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_tool_returns_file_contents() {
        let root = temp_root();
        let path = root.join("hello.txt");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "hello, world").unwrap();
        let tool = ReadTool { root: root.clone() };
        let output = tool
            .execute(&json!({"path": "hello.txt"}))
            .expect("read succeeds");
        assert!(output.contains("hello, world"));
    }

    #[test]
    fn read_tool_truncates_large_files() {
        let root = temp_root();
        let path = root.join("big.txt");
        std::fs::write(&path, "x".repeat(60_000)).unwrap();
        let tool = ReadTool { root };
        let output = tool
            .execute(&json!({"path": "big.txt"}))
            .expect("read succeeds");
        assert!(output.contains("(truncated)"));
    }

    #[test]
    fn read_tool_rejects_missing_path() {
        let tool = ReadTool {
            root: temp_root(),
        };
        let result = tool.execute(&json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn bash_tool_runs_in_root_and_returns_output() {
        let root = temp_root();
        let tool = BashTool {
            root: root.clone(),
            ..BashTool::default()
        };
        let output = tool
            .execute(&json!({"command": "echo hi && pwd"}))
            .expect("bash succeeds");
        assert!(output.contains("hi"));
        // The pwd output should mention the temp root.
        assert!(output.contains(&root.display().to_string()));
    }

    #[test]
    fn bash_tool_surfaces_nonzero_exit_as_error() {
        let tool = BashTool {
            root: temp_root(),
            ..BashTool::default()
        };
        let result = tool.execute(&json!({"command": "exit 7"}));
        assert!(result.is_err());
    }

    #[test]
    fn read_tool_resolves_absolute_paths() {
        let root = temp_root();
        let path = root.join("abs.txt");
        std::fs::write(&path, "absolute content").unwrap();
        let tool = ReadTool { root };
        let output = tool
            .execute(&json!({"path": path.to_str().unwrap()}))
            .expect("read succeeds");
        assert!(output.contains("absolute content"));
    }
}
