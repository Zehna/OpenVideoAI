pub mod detector;
pub mod parser;

pub use detector::{detect_tool, find_executable, ToolStatus};
pub use parser::parse_tool_version;
