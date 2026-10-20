use super::{Association, BranchId};
use crate::contributions::lore::Deployment;
use anyhow::{Context, Result};

/// Public native checkout coordinates only. Never a command or credential.
#[derive(Clone, Debug)]
pub(super) struct CheckoutDescriptor {
    pub remote: url::Url,
    pub repository: super::RepositoryId,
    pub branch: BranchId,
}
impl CheckoutDescriptor {
    pub fn new(deployment: &Deployment, association: &Association) -> Result<Self> {
        let mut remote = deployment.endpoint.clone();
        remote.set_path(&format!("/{}", association.repository));
        Ok(Self {
            remote,
            repository: association.repository.clone(),
            branch: association
                .default_branch
                .clone()
                .context("verified workspace branch missing")?,
        })
    }
    pub fn assignments(&self) -> Vec<u8> {
        format!("COMPUTER_WORKSPACE_REMOTE={}\nCOMPUTER_WORKSPACE_REPOSITORY={}\nCOMPUTER_WORKSPACE_BRANCH={}\n",
            literal(self.remote.as_str()), literal(self.repository.as_str()), literal(self.branch.as_str())).into_bytes()
    }
}
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) const INSTALL_AND_RETRY: &str = "umask 077; mkdir -p /sandbox/home/.config/computer; tmp=$(mktemp /sandbox/home/.config/computer/.workspace.XXXXXX); trap 'rm -f \"$tmp\"' EXIT; cat >\"$tmp\"; chmod 600 \"$tmp\"; mv -f \"$tmp\" /sandbox/home/.config/computer/workspace.env; exec /usr/local/bin/computer-workspace --retry";

#[cfg(test)]
mod tests {
    #[test]
    fn shell_literals_round_trip_without_executing_input() {
        use std::process::Command;
        let value = "space ' quote\n$(exit 87); `exit 88` \\ end";
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("workspace.env");
        std::fs::write(
            &file,
            format!("COMPUTER_WORKSPACE_REMOTE={}\n", super::literal(value)),
        )
        .unwrap();
        let output = Command::new("/bin/sh")
            .args([
                "-c",
                ". \"$1\"; printf '%s' \"$COMPUTER_WORKSPACE_REMOTE\"",
                "test",
            ])
            .arg(file)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, value.as_bytes());
    }
}
