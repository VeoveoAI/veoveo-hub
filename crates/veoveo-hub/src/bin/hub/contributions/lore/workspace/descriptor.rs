use super::{Association, BranchId};
use crate::contributions::lore::Deployment;
use anyhow::{Context, Result};

/// Public personal-repository coordinates only. Never a command or credential.
#[derive(Clone, Debug)]
pub(super) struct WorkspaceDescriptor {
    pub remote: url::Url,
    pub repository: super::RepositoryId,
    pub branch: BranchId,
}
impl WorkspaceDescriptor {
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

pub(super) const INSTALL_DESCRIPTOR: &str = "umask 077; mkdir -p /sandbox/home/.config/computer; tmp=$(mktemp /sandbox/home/.config/computer/.workspace.XXXXXX); trap 'rm -f \"$tmp\"' EXIT; cat >\"$tmp\"; chmod 600 \"$tmp\"; mv -f \"$tmp\" /sandbox/home/.config/computer/workspace.env";

#[cfg(test)]
mod tests {
    #[test]
    fn descriptor_installation_only_delivers_public_coordinates() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("computer");
        let command = super::INSTALL_DESCRIPTOR.replace(
            "/sandbox/home/.config/computer",
            destination.to_str().unwrap(),
        );
        let bytes = b"COMPUTER_WORKSPACE_REMOTE='grpcs://native.example:41337/repository'\n";
        let mut child = Command::new("/bin/sh")
            .args(["-ec", &command])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(
            std::fs::read(destination.join("workspace.env")).unwrap(),
            bytes
        );
        assert_eq!(std::fs::read_dir(destination).unwrap().count(), 1);
    }

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
