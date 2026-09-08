use super::parser::parse_tool_version;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

impl ToolStatus {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            path: None,
            version: None,
        }
    }
}

/// Locate an executable from an optional custom path or by traversing system PATH.
pub fn find_executable(tool_name: &str, custom_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(custom) = custom_path {
        if custom.is_file() {
            return Some(custom.to_path_buf());
        }
    }

    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        // Direct match
        let candidate = dir.join(tool_name);
        if candidate.is_file() {
            return Some(candidate);
        }

        // Windows executable extensions
        #[cfg(target_os = "windows")]
        {
            let candidate_exe = dir.join(format!("{}.exe", tool_name));
            if candidate_exe.is_file() {
                return Some(candidate_exe);
            }
        }
    }

    None
}

/// Detect availability, canonical path, and version of a given binary tool.
/// Executes `<binary> -version` safely without panicking on failure.
pub fn detect_tool(tool_name: &str, custom_path: Option<&Path>) -> ToolStatus {
    let exe_path = match find_executable(tool_name, custom_path) {
        Some(path) => path,
        None => {
            tracing::warn!(tool = tool_name, "Executable not found on system PATH");
            return ToolStatus::unavailable();
        }
    };

    let canonical_path_str = exe_path
        .canonicalize()
        .unwrap_or_else(|_| exe_path.clone())
        .to_string_lossy()
        .to_string();

    // Clean Windows verbatim path prefix (\\?\) for clean display
    let display_path = canonical_path_str
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical_path_str)
        .to_string();

    let output = match Command::new(&exe_path).arg("-version").output() {
        Ok(out) => out,
        Err(e) => {
            tracing::warn!(tool = tool_name, path = %display_path, error = %e, "Failed to execute tool");
            return ToolStatus::unavailable();
        }
    };

    if !output.status.success() {
        tracing::warn!(
            tool = tool_name,
            path = %display_path,
            exit_code = ?output.status.code(),
            "Tool returned non-zero exit code"
        );
        return ToolStatus::unavailable();
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let version = match parse_tool_version(tool_name, &stdout_str) {
        Some(ver) => ver,
        None => {
            // Fallback to stderr if version info is on stderr
            let stderr_str = String::from_utf8_lossy(&output.stderr);
            match parse_tool_version(tool_name, &stderr_str) {
                Some(ver) => ver,
                None => {
                    tracing::warn!(tool = tool_name, path = %display_path, "Could not parse version from output");
                    return ToolStatus {
                        available: false,
                        path: Some(display_path),
                        version: None,
                    };
                }
            }
        }
    };

    tracing::info!(tool = tool_name, version = %version, path = %display_path, "Successfully detected tool");

    ToolStatus {
        available: true,
        path: Some(display_path),
        version: Some(version),
    }
}
