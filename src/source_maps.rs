use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMapOutput<'a> {
    Suppressed,
    Inline,
    File(PathBuf, &'a Path),
}
