use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};

/// What to search
pub enum Input {
    Paths(Vec<PathBuf>),
    Text(String),
}

impl Input {
    /// Paths given on the command line, else `--text`, else stdin when piped, else `.`
    pub fn new(paths: Vec<PathBuf>, text: Option<String>) -> io::Result<Self> {
        if let Some(text) = text {
            return Ok(Self::Text(text));
        }
        if !paths.is_empty() {
            return Ok(Self::Paths(paths));
        }
        if stdin_is_readable() {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            return Ok(Self::Text(text));
        }
        Ok(Self::Paths(vec![PathBuf::from(".")]))
    }
}

/// Like ripgrep, only read stdin when it is a pipe or a file (not a terminal or `/dev/null`)
fn stdin_is_readable() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        fs::metadata("/dev/stdin").is_ok_and(|m| m.is_file() || m.file_type().is_fifo())
    }
    #[cfg(not(unix))]
    {
        use std::io::IsTerminal;
        !io::stdin().is_terminal()
    }
}
