use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceFormat {
    LegacyBinary,
    ChampSim,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredTrace {
    pub path: PathBuf,
    pub format: TraceFormat,
}

pub fn detect_trace_format(path: impl AsRef<Path>) -> Option<TraceFormat> {
    let name = path.as_ref().file_name()?.to_str()?;
    if name.ends_with(".champsimtrace") || name.ends_with(".champsimtrace.xz") {
        Some(TraceFormat::ChampSim)
    } else if name.ends_with(".bin") || name.ends_with(".bin.gz") {
        Some(TraceFormat::LegacyBinary)
    } else {
        None
    }
}

pub fn discover_traces(directory: impl AsRef<Path>) -> std::io::Result<Vec<DiscoveredTrace>> {
    let mut traces: Vec<_> = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            detect_trace_format(&path).map(|format| DiscoveredTrace { path, format })
        })
        .collect();
    traces.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
    Ok(traces)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_extensions() {
        assert_eq!(
            detect_trace_format("x.bin"),
            Some(TraceFormat::LegacyBinary)
        );
        assert_eq!(
            detect_trace_format("x.bin.gz"),
            Some(TraceFormat::LegacyBinary)
        );
        assert_eq!(
            detect_trace_format("x.champsimtrace"),
            Some(TraceFormat::ChampSim)
        );
        assert_eq!(
            detect_trace_format("x.champsimtrace.xz"),
            Some(TraceFormat::ChampSim)
        );
        assert_eq!(detect_trace_format("x.csv"), None);
    }
}
