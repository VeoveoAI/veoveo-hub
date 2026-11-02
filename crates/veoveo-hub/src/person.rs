//! The people who signed in to the Hub, by tenant, as their identity provider last named
//! them: display name and email. The sign-in callback records each person at every
//! sign-in, so a person who signed in can be named by email and is shown by email and
//! name. An email belongs to one person of a tenant; a sign-in with another person's
//! email takes it over. A contribution that holds a tenant's groups names the groups a
//! person belongs to (`PersonGroups`).
use std::{fmt, sync::Arc};

use anyhow::{Result, ensure};

use serde::Serialize;
use veoveo_types::{PrincipalId, TenantId};

use crate::contract::PrincipalDisplayName;

/// An email address in lowercase: a local part and a domain around one `@`, with no
/// whitespace or control character, at most 254 bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Email(String);

/// The text is no email address.
#[derive(Debug, Clone, thiserror::Error)]
#[error("an email address is a local part and a domain around one @, at most 254 bytes")]
pub struct EmailError;

impl Email {
    /// The address `value` names, trimmed and in lowercase.
    pub fn parse(value: &str) -> Result<Self, EmailError> {
        let value = value.trim().to_lowercase();
        let parts = value.split('@').collect::<Vec<_>>();
        let admitted = value.len() <= 254
            && matches!(parts.as_slice(), [local, domain] if !local.is_empty() && !domain.is_empty())
            && !value.chars().any(|c| c.is_whitespace() || c.is_control());
        if admitted {
            Ok(Self(value))
        } else {
            Err(EmailError)
        }
    }

    /// The address an identity provider's claims name: its `email` claim, or its
    /// `preferred_username` when that is an address; none without either.
    pub fn from_claims(email: Option<&str>, preferred_username: Option<&str>) -> Option<Self> {
        email
            .and_then(|email| Self::parse(email).ok())
            .or_else(|| preferred_username.and_then(|name| Self::parse(name).ok()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A group of a person's tenant that they belong to, by the name the contribution that
/// holds the tenant's groups gives it, with their role in it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PersonGroup {
    pub name: String,
    pub role: PersonGroupRole,
}

/// A person's role in a group: one of its owners, or a member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PersonGroupRole {
    Owner,
    Member,
}

impl fmt::Display for PersonGroupRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Owner => "owner",
            Self::Member => "member",
        })
    }
}

/// The groups of a tenant a person belongs to in a catalog revision, as the contribution
/// that holds the tenant's groups knows them.
pub trait PersonGroups: fmt::Debug + Send + Sync {
    fn groups(
        &self,
        catalog: &crate::HubCatalog,
        tenant: &TenantId,
        principal: &PrincipalId,
    ) -> Result<Vec<PersonGroup>>;
}

impl crate::HubState {
    /// Name people's groups with `groups`. Without it, a person belongs to no group the
    /// Hub can name.
    pub fn bind_person_groups(mut self, groups: Arc<dyn PersonGroups>) -> Result<Self> {
        ensure!(self.person_groups.is_none(), "person groups already bound");
        self.person_groups = Some(groups);
        Ok(self)
    }
}

/// A person of a tenant as their last sign-in named them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub tenant: TenantId,
    pub principal: PrincipalId,
    pub name: PrincipalDisplayName,
    pub email: Option<Email>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_email_is_one_lowercase_address() {
        assert_eq!(
            Email::parse("  Alex.Rozgo@Example.COM ").unwrap().as_str(),
            "alex.rozgo@example.com"
        );
        for refused in [
            "",
            "alex",
            "@example.com",
            "alex@",
            "a@b@c",
            "al ex@example.com",
            "a\u{7}@b",
        ] {
            assert!(Email::parse(refused).is_err(), "{refused:?}");
        }
        // At most 254 bytes.
        assert!(Email::parse(&format!("{}@b", "a".repeat(253))).is_err());
        assert!(Email::parse(&format!("{}@b", "a".repeat(252))).is_ok());
    }

    #[test]
    fn the_claims_name_an_email_or_an_address_as_the_preferred_username() {
        let email = |value: &str| Some(Email::parse(value).unwrap());
        assert_eq!(
            Email::from_claims(Some("A@b.c"), Some("x@y.z")),
            email("a@b.c")
        );
        assert_eq!(Email::from_claims(None, Some("X@Y.z")), email("x@y.z"));
        assert_eq!(
            Email::from_claims(Some("not an address"), Some("x@y.z")),
            email("x@y.z")
        );
        assert_eq!(Email::from_claims(None, Some("alex")), None);
        assert_eq!(Email::from_claims(None, None), None);
    }
}
