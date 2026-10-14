//! Runs each simplification in a child copy of this executable, so a call
//! that runs too long can be killed and a crash in CoBRA cannot take down the
//! server. The parent writes a JSON `Request` to the worker's stdin and reads
//! a JSON `Result<Simplification, String>` from its stdout.

use std::io::{self, Read, Write};
use std::process::{ExitCode, Stdio};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::cobra::{self, Request, Simplification};

pub const FLAG: &str = "--worker";

/// Entry point of the worker process.
pub fn run() -> ExitCode {
    let mut input = Vec::new();
    if let Err(e) = io::stdin().read_to_end(&mut input) {
        eprintln!("cobra-mcp worker: reading request: {e}");
        return ExitCode::FAILURE;
    }
    let request: Request = match serde_json::from_slice(&input) {
        Ok(request) => request,
        Err(e) => {
            eprintln!("cobra-mcp worker: invalid request: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = cobra::simplify(&request);
    let mut stdout = io::stdout().lock();
    if let Err(e) = serde_json::to_writer(&mut stdout, &result)
        .map_err(io::Error::from)
        .and_then(|()| stdout.flush())
    {
        eprintln!("cobra-mcp worker: writing result: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Runs `request` in a worker process, killing it after `timeout`.
pub async fn simplify(request: &Request, timeout: Duration) -> Result<Simplification, String> {
    let input = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| format!("locating cobra-mcp executable: {e}"))?;
    let mut child = Command::new(exe)
        .arg(FLAG)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("starting CoBRA worker: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();

    // Dropping this future on timeout drops `child`, which kills the worker.
    let run = async move {
        stdin.write_all(&input).await?;
        drop(stdin);
        child.wait_with_output().await
    };
    let output = match tokio::time::timeout(timeout, run).await {
        Ok(output) => output.map_err(|e| format!("running CoBRA worker: {e}"))?,
        Err(_) => return Err(format!("CoBRA timed out after {} ms", timeout.as_millis())),
    };
    if !output.status.success() {
        return Err(format!("CoBRA worker failed ({})", output.status));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("reading CoBRA worker result: {e}"))?
}
