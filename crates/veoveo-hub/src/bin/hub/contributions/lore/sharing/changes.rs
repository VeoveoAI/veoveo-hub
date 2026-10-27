//! What owners change in the Lore section: who may reach a repository, the groups of a
//! tenant and their memberships. Each change is made as one caller, who must own what
//! they change and must still own it afterwards, so neither a repository nor a group is
//! ever left without an owner by a change of these.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use veoveo_types::{PrincipalId, TenantId};

use super::super::{
    Assignment, DeploymentId, Group, GroupId, GroupName, GroupRole, Lore, Member, NativeAction,
    Repository, RepositoryId, Subject,
};

/// An assignment's level: `member`, an assignment without actions, or `owner`, which
/// holds the `owner` action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Level {
    Member,
    Owner,
}

impl Level {
    /// The level an assignment's actions show: `owner` when they hold the `owner` action.
    /// An administrator may publish other actions; they are listed beside the level.
    pub(crate) fn of(actions: &BTreeSet<NativeAction>) -> Self {
        if actions.contains(&NativeAction::Owner) {
            Self::Owner
        } else {
            Self::Member
        }
    }

    fn actions(self) -> BTreeSet<NativeAction> {
        match self {
            Self::Member => BTreeSet::new(),
            Self::Owner => BTreeSet::from([NativeAction::Owner]),
        }
    }
}

/// What an owner sets for a person or a group: `member` or `owner`, or `none`, which
/// removes the assignment or the membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Setting {
    Member,
    Owner,
    None,
}

/// Whom a change names: a person who signed in, or a group of the caller's tenant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    Person(PrincipalId),
    Group(GroupName),
}

/// Why a change, or a read, is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Refusal {
    #[error("no repository of this ID is registered in the caller's tenant")]
    UnknownRepository,
    #[error("the caller holds no grant on the repository")]
    NotGranted,
    #[error("the caller does not own the repository")]
    NotRepositoryOwner,
    #[error("the caller's tenant has no group of this name")]
    UnknownGroup,
    #[error("the caller does not own the group")]
    NotGroupOwner,
    #[error("the caller's tenant has a group of this name")]
    GroupTaken,
    #[error("the change would leave the caller without owner on the repository")]
    KeepRepositoryOwner,
    #[error("the change would leave the caller without the owner role in the group")]
    KeepGroupOwner,
}

/// The person who calls, in their tenant.
#[derive(Clone, Copy)]
pub(crate) struct Caller<'a> {
    pub tenant: &'a TenantId,
    pub principal: &'a PrincipalId,
}

impl Lore {
    /// The deployment's repository of this ID, registered in the tenant.
    pub(crate) fn repository_of(
        &self,
        deployment: &DeploymentId,
        id: &RepositoryId,
        tenant: &TenantId,
    ) -> Result<&Repository, Refusal> {
        self.repositories
            .iter()
            .find(|r| &r.deployment == deployment && &r.id == id && &r.tenant == tenant)
            .ok_or(Refusal::UnknownRepository)
    }

    /// The tenant's group of this name.
    pub(crate) fn group_named(&self, tenant: &TenantId, name: &GroupName) -> Option<&Group> {
        self.groups
            .iter()
            .find(|g| &g.tenant == tenant && &g.name == name)
    }

    /// Every assignment on the deployment's repository.
    pub(crate) fn access<'a>(
        &'a self,
        deployment: &'a DeploymentId,
        repository: &'a RepositoryId,
    ) -> impl Iterator<Item = &'a Assignment> {
        self.assignments
            .iter()
            .filter(move |a| &a.deployment == deployment && &a.repository == repository)
    }

    /// Whether the caller's actions on the repository, their own and their groups', hold
    /// `owner`.
    fn owns(
        &self,
        deployment: &DeploymentId,
        repository: &RepositoryId,
        caller: Caller<'_>,
    ) -> bool {
        self.actions(deployment, repository, caller.tenant, caller.principal)
            .is_some_and(|actions| actions.contains(&NativeAction::Owner))
    }

    /// Who may read a repository's access: a holder of a grant on it.
    pub(crate) fn readable(
        &self,
        deployment: &DeploymentId,
        repository: &RepositoryId,
        caller: Caller<'_>,
    ) -> Result<(), Refusal> {
        self.repository_of(deployment, repository, caller.tenant)?;
        self.actions(deployment, repository, caller.tenant, caller.principal)
            .map(|_| ())
            .ok_or(Refusal::NotGranted)
    }

    /// Set `subject`'s assignment on the repository to the level `setting` names,
    /// replacing whatever actions it held, or remove it, as `caller`, an owner of the
    /// repository who stays one. Answers whether the section changed.
    pub(crate) fn set_access(
        &mut self,
        deployment: &DeploymentId,
        repository: &RepositoryId,
        caller: Caller<'_>,
        subject: &Named,
        setting: Setting,
    ) -> Result<bool, Refusal> {
        self.repository_of(deployment, repository, caller.tenant)?;
        if !self.owns(deployment, repository, caller) {
            return Err(Refusal::NotRepositoryOwner);
        }
        let subject = match subject {
            Named::Person(principal) => Subject::Principal(principal.clone()),
            Named::Group(name) => Subject::Group(
                self.group_named(caller.tenant, name)
                    .ok_or(Refusal::UnknownGroup)?
                    .id
                    .clone(),
            ),
        };
        let level = match setting {
            Setting::Member => Some(Level::Member),
            Setting::Owner => Some(Level::Owner),
            Setting::None => None,
        };
        let existing = self.assignments.iter().position(|a| {
            &a.deployment == deployment && &a.repository == repository && a.subject == subject
        });
        let before = self.assignments.clone();
        match (existing, level) {
            (None, None) => return Ok(false),
            (Some(index), None) => {
                self.assignments.remove(index);
            }
            (Some(index), Some(level)) => {
                if self.assignments[index].actions == level.actions() {
                    return Ok(false);
                }
                self.assignments[index].actions = level.actions();
            }
            (None, Some(level)) => self.assignments.push(Assignment {
                deployment: deployment.clone(),
                repository: repository.clone(),
                tenant: caller.tenant.clone(),
                subject,
                actions: level.actions(),
            }),
        }
        if !self.owns(deployment, repository, caller) {
            self.assignments = before;
            return Err(Refusal::KeepRepositoryOwner);
        }
        Ok(true)
    }

    /// Remove the deployment's repository and every assignment on it, as `caller`, an
    /// owner of it, when Lore deletes it. A repository the section does not register
    /// leaves nothing to remove. Answers whether the section changed.
    pub(crate) fn delete_repository(
        &mut self,
        deployment: &DeploymentId,
        repository: &RepositoryId,
        caller: Caller<'_>,
    ) -> Result<bool, Refusal> {
        let Some(registered) = self
            .repositories
            .iter()
            .find(|r| &r.deployment == deployment && &r.id == repository)
        else {
            return Ok(false);
        };
        if &registered.tenant != caller.tenant || !self.owns(deployment, repository, caller) {
            return Err(Refusal::NotRepositoryOwner);
        }
        self.repositories
            .retain(|r| !(&r.deployment == deployment && &r.id == repository));
        self.assignments
            .retain(|a| !(&a.deployment == deployment && &a.repository == repository));
        Ok(true)
    }

    /// Create the tenant's group `name`, of `id`, with the caller as its one member, its
    /// owner.
    pub(crate) fn create_group(
        &mut self,
        caller: Caller<'_>,
        name: &GroupName,
        id: GroupId,
    ) -> Result<&Group, Refusal> {
        if self.group_named(caller.tenant, name).is_some() {
            return Err(Refusal::GroupTaken);
        }
        self.groups.push(Group {
            id,
            tenant: caller.tenant.clone(),
            name: name.clone(),
            members: vec![Member {
                principal: caller.principal.clone(),
                role: GroupRole::Owner,
            }],
        });
        Ok(self.groups.last().expect("the group just pushed"))
    }

    /// Set `person`'s role in the tenant's group `name` to the one `setting` names, or
    /// remove their membership, as `caller`, an owner of the group who stays one. Answers
    /// whether the section changed.
    pub(crate) fn set_membership(
        &mut self,
        caller: Caller<'_>,
        name: &GroupName,
        person: &PrincipalId,
        setting: Setting,
    ) -> Result<bool, Refusal> {
        let group = self
            .groups
            .iter_mut()
            .find(|g| g.tenant == *caller.tenant && &g.name == name)
            .ok_or(Refusal::UnknownGroup)?;
        if group.role(caller.principal) != Some(GroupRole::Owner) {
            return Err(Refusal::NotGroupOwner);
        }
        let role = match setting {
            Setting::Member => Some(GroupRole::Member),
            Setting::Owner => Some(GroupRole::Owner),
            Setting::None => None,
        };
        if person == caller.principal && role != Some(GroupRole::Owner) {
            return Err(Refusal::KeepGroupOwner);
        }
        let existing = group.members.iter().position(|m| &m.principal == person);
        match (existing, role) {
            (None, None) => Ok(false),
            (Some(index), None) => {
                group.members.remove(index);
                Ok(true)
            }
            (Some(index), Some(role)) => {
                let changed = group.members[index].role != role;
                group.members[index].role = role;
                Ok(changed)
            }
            (None, Some(role)) => {
                group.members.push(Member {
                    principal: person.clone(),
                    role,
                });
                Ok(true)
            }
        }
    }
}
