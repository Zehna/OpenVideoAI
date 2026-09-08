use regex::Regex;
use std::sync::LazyLock;

static FFMPEG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)ffmpeg\s+version\s+([^\s,]+)").expect("invalid ffmpeg regex")
});

static FFPROBE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)ffprobe\s+version\s+([^\s,]+)").expect("invalid ffprobe regex")
});

/// Safely parse version string from `ffmpeg -version` or `ffprobe -version` standard output.
pub fn parse_tool_version(tool_name: &str, output: &str) -> Option<String> {
    let re = match tool_name.to_lowercase().as_str() {
        "ffmpeg" => &*FFMPEG_REGEX,
        "ffprobe" => &*FFPROBE_REGEX,
        _ => return None,
    };

    re.captures(output).and_then(|caps| caps.get(1)).map(|m| m.as_str().to_string())
}
