//! Bounded stdout/stderr for commands inside the existing native Exec process group.
//! Native Exec owns deadline/cancellation of the helper AND ordinary descendants.
//! Do not create a child process group/session here: it would escape that cleanup.
use anyhow::{Context, Result, ensure};
use std::{collections::VecDeque, process::Stdio};
use tokio::io::{AsyncRead, AsyncReadExt as _};

pub struct Output {
    pub code: Option<i32>,
    pub bytes: Vec<u8>,
    pub truncated: bool,
    pub stderr: Vec<u8>,
    pub stderr_truncated: bool,
}
async fn drain(mut stream: impl AsyncRead + Unpin, limit: usize) -> Result<(Vec<u8>, bool)> {
    let mut bytes = VecDeque::new();
    let mut truncated = false;
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok((bytes.into_iter().collect(), truncated));
        }
        for byte in &buffer[..count] {
            if bytes.len() == limit {
                bytes.pop_front();
                truncated = true;
            }
            bytes.push_back(*byte);
        }
    }
}
pub async fn run(mut command: tokio::process::Command, limit: usize) -> Result<Output> {
    ensure!(limit > 0, "command output bound must be positive");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("launch native recipe command")?;
    let stdout = child.stdout.take().context("command stdout missing")?;
    let stderr = child.stderr.take().context("command stderr missing")?;
    let (stdout_result, stderr_result, status) = tokio::join!(
        drain(stdout, limit),
        drain(stderr, limit.min(16 * 1024)),
        child.wait()
    );
    // Always finish the foreground child wait before propagating a drain error.
    // Native Exec remains the outer deadline and descendant cancellation owner.
    let status = status?;
    let (bytes, truncated) = stdout_result?;
    let (stderr, stderr_truncated) = stderr_result?;
    Ok(Output {
        code: status.code(),
        bytes,
        truncated,
        stderr,
        stderr_truncated,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn machine_readable_stdout_is_not_corrupted_by_stderr_diagnostics() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '{\"deployment\":\"candidate\"}'; printf 'native diagnostic' >&2",
        ]);
        let output = run(command, 1024).await.unwrap();
        assert_eq!(output.code, Some(0));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.bytes).unwrap()["deployment"],
            "candidate"
        );
        assert_eq!(output.stderr, b"native diagnostic");
    }
    #[tokio::test]
    async fn command_output_keeps_only_bounded_tail() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", "printf 123456789"]);
        let output = run(command, 4).await.unwrap();
        assert_eq!(output.bytes, b"6789");
        assert!(output.truncated);
    }
}
