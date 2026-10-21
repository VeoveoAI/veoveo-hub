//! Typed source-adapter persistence is atomic with generic reservation. No expired
//! caller JWT, download URL or external group/role assertion becomes durable authority.
use super::types::*;
pub(super) type ControlDocument = sqlx::types::Json<veoveo_hub::control_plane::HubControlPlane>;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, types::Json};
use std::{collections::BTreeMap, sync::Mutex};
use veoveo_hub::{
    AuthenticatedSubject,
    activation::*,
    execution::EnvironmentOwner,
    store::{HubStoreError, TransactionAppend},
};
use veoveo_hub_sdk::{
    HubProfileId, OAuthClientId, Principal, PrincipalKind, TokenIssuer, TokenSubject,
};
use veoveo_types::{BuildId, PrincipalId, Sha256Digest, TenantId, WorkContextId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceAuthority {
    pub principal: PrincipalId,
    pub tenant: TenantId,
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    pub client: OAuthClientId,
    pub profile: HubProfileId,
    pub work_context: WorkContextId,
}
impl SourceAuthority {
    pub fn capture(subject: &AuthenticatedSubject, profile: HubProfileId) -> Result<Self> {
        ensure!(
            subject.principal.kind == PrincipalKind::User && subject.delegation.is_none(),
            ActivationError::Forbidden
        );
        Ok(Self {
            principal: subject.principal.id.clone(),
            tenant: subject
                .principal
                .tenant
                .clone()
                .ok_or(ActivationError::Forbidden)?,
            issuer: subject.principal.issuer.clone(),
            subject: subject.principal.subject.clone(),
            client: subject.access_token.oauth_client_id.clone(),
            profile,
            work_context: subject.authority.work_context.clone(),
        })
    }
    pub fn background_audit(&self) -> veoveo_audit_contract::AuditContext {
        veoveo_audit_contract::AuditContext {
            actor: veoveo_audit_contract::AuditActor {
                principal: self.principal.clone(),
                kind: veoveo_audit_contract::AuditPrincipalKind::User,
                tenant: Some(self.tenant.clone()),
                work_context: Some(self.work_context.clone()),
            },
            via: None,
            request: veoveo_audit_contract::AuditRequest::background(),
        }
    }
    /// Explicit native assignments use tenant+principal. External roles/groups are
    /// deliberately NOT reconstructed as fresh authentication or broader authority.
    pub fn native_principal(&self) -> Principal {
        Principal {
            id: self.principal.clone(),
            kind: PrincipalKind::User,
            issuer: self.issuer.clone(),
            subject: self.subject.clone(),
            tenant: Some(self.tenant.clone()),
            groups: Default::default(),
            group_roles: Default::default(),
            roles: Default::default(),
            scopes: Default::default(),
            data_labels: Default::default(),
            assurances: Default::default(),
            authenticated_at: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRecord {
    pub component: ComponentDefinition,
    pub authority: SourceAuthority,
    /// First resolved immutable native revision for this accepted generation.
    #[serde(default)]
    pub resolved_revision: Option<NativeRevision>,
}

pub struct DefinitionAppend {
    owner: EnvironmentOwner,
    environment: veoveo_hub_sdk::EnvironmentId,
    idempotency: ApplyIdempotencyKey,
    authority: SourceAuthority,
    definition: EnvironmentDefinition,
    origin: super::definition_source::DefinitionOrigin,
    accepted: Mutex<Vec<AcceptedApply>>,
}
impl DefinitionAppend {
    pub fn new(
        subject: &AuthenticatedSubject,
        profile: HubProfileId,
        request: ApplyEnvironment,
    ) -> Result<Self> {
        let authority = SourceAuthority::capture(subject, profile)?;
        Ok(Self {
            owner: owner(&authority.tenant, &authority.principal),
            environment: request.definition.id,
            idempotency: request.idempotency,
            authority,
            definition: request.definition,
            origin: super::definition_source::DefinitionOrigin::Local,
            accepted: Mutex::new(Vec::new()),
        })
    }
    pub(super) fn from_follow(
        state: super::definition_source::DefinitionSourceState,
        revision: NativeRevision,
        definition: EnvironmentDefinition,
    ) -> Result<Self> {
        ensure!(
            state.environment == definition.id,
            ActivationError::InvalidDefinition
        );
        let bytes = serde_json::to_vec(&(state.generation, &revision))?;
        Ok(Self {
            owner: state.owner.clone(),
            environment: state.environment,
            idempotency: ApplyIdempotencyKey::parse(format!(
                "definition-follow-{}",
                hex::encode(Sha256::digest(bytes))
            ))?,
            authority: state.authority.clone(),
            definition,
            origin: super::definition_source::DefinitionOrigin::Follow {
                expected: Box::new(state),
                revision,
            },
            accepted: Mutex::new(Vec::new()),
        })
    }
    pub async fn replay_on(
        &self,
        connection: &mut PgConnection,
    ) -> Result<Option<Vec<AcceptedApply>>> {
        let row = sqlx::query_file!(
            "src/bin/hub/activation/apply_read.sql",
            self.owner.tenant.as_str(),
            self.owner.principal.as_str(),
            self.environment.as_uuid(),
            self.idempotency.as_str()
        )
        .fetch_optional(connection)
        .await?;
        row.map(|r| {
            ensure!(
                r.document.0.definition == self.definition,
                ActivationError::IdempotencyConflict
            );
            Ok(r.document.0.accepted)
        })
        .transpose()
    }
    pub fn accepted(&self) -> Vec<AcceptedApply> {
        self.accepted
            .lock()
            .expect("definition result mutex")
            .clone()
    }
}
impl TransactionAppend for DefinitionAppend {
    async fn append(&self, connection: &mut PgConnection) -> Result<(), HubStoreError> {
        if let Some(replayed) = self
            .replay_on(connection)
            .await
            .map_err(HubStoreError::Activation)?
        {
            *self.accepted.lock().expect("definition result mutex") = replayed;
            return Ok(());
        }
        let source_state = super::definition_source::prepare_on(
            connection,
            &self.owner,
            self.environment,
            &self.authority,
            &self.origin,
        )
        .await
        .map_err(HubStoreError::Activation)?;
        let mut accepted = Vec::new();
        for (component, definition) in &self.definition.components {
            if let Some(current) = sqlx::query_file!(
                "src/bin/hub/activation/source_current.sql",
                self.owner.tenant.as_str(),
                self.owner.principal.as_str(),
                self.environment.as_uuid(),
                component.as_str()
            )
            .fetch_optional(&mut *connection)
            .await?
                && current.document.0.component == *definition
            {
                // Same desired source mode/recipe/template: a label/base edit
                // does not create new workers or revive historical digest matches.
                let control = sqlx::query_file!("src/bin/hub/activation/definition_control.sql")
                    .fetch_one(&mut *connection)
                    .await?
                    .control_plane
                    .0;
                current
                    .accepted
                    .0
                    .admit(&control)
                    .map_err(|e| HubStoreError::Activation(e.into()))?;
                accepted.push(current.accepted.0);
                continue;
            }
            let key = self
                .owner
                .activation_key(self.environment, component.clone());
            let bytes = serde_json::to_vec(&(self.idempotency.as_str(), component))
                .map_err(|e| HubStoreError::Append(Box::new(e)))?;
            let idempotency = ApplyIdempotencyKey::parse(format!(
                "apply-{}",
                hex::encode(Sha256::digest(&bytes))
            ))
            .map_err(|e| HubStoreError::Append(Box::new(e)))?;
            let item = reserve_on(
                connection,
                key,
                idempotency,
                definition.clone(),
                self.authority.clone(),
            )
            .await
            .map_err(HubStoreError::Activation)?;
            accepted.push(item);
        }
        let receipt = ApplyReceipt {
            definition: self.definition.clone(),
            accepted: accepted.clone(),
        };
        sqlx::query_file!(
            "src/bin/hub/activation/apply_insert.sql",
            self.owner.tenant.as_str(),
            self.owner.principal.as_str(),
            self.environment.as_uuid(),
            self.idempotency.as_str(),
            Json(&receipt) as _
        )
        .execute(&mut *connection)
        .await?;
        super::definition_source::persist_on(connection, &source_state)
            .await
            .map_err(HubStoreError::Activation)?;
        *self.accepted.lock().expect("definition result mutex") = accepted;
        Ok(())
    }
}
pub async fn reserve_on(
    connection: &mut PgConnection,
    key: ActivationKey,
    idempotency: ApplyIdempotencyKey,
    component: ComponentDefinition,
    authority: SourceAuthority,
) -> Result<AcceptedApply> {
    reserve_resolved_on(connection, key, idempotency, component, authority, None).await
}
pub async fn reserve_resolved_on(
    connection: &mut PgConnection,
    key: ActivationKey,
    idempotency: ApplyIdempotencyKey,
    component: ComponentDefinition,
    authority: SourceAuthority,
    resolved_revision: Option<NativeRevision>,
) -> Result<AcceptedApply> {
    let bytes = serde_json::to_vec(&(&component, &resolved_revision))?;
    let inputs = BuildInputs {
        runtime_template: component.runtime_template.clone(),
        definition_digest: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
        expected_source: match &component.source {
            SourceSelector::LocalSnapshot { snapshot } => Some(snapshot.clone()),
            SourceSelector::Lore { .. } => None,
        },
        recipe: component.recipe.clone(),
    };
    let accepted = ActivationStore::reserve_on(connection, key, idempotency, inputs).await?;
    let record = SourceRecord {
        component,
        authority,
        resolved_revision,
    };
    sqlx::query_file!(
        "src/bin/hub/activation/source_insert.sql",
        accepted.build.as_uuid(),
        Json(&record) as _
    )
    .execute(&mut *connection)
    .await?;
    let stored = read_on(connection, accepted.build).await?;
    ensure!(stored == record, ActivationError::IdempotencyConflict);
    Ok(accepted)
}
pub async fn read_on(connection: &mut PgConnection, build: BuildId) -> Result<SourceRecord> {
    Ok(
        sqlx::query_file!("src/bin/hub/activation/source_read.sql", build.as_uuid())
            .fetch_one(connection)
            .await?
            .document
            .0,
    )
}

pub async fn definitions(
    connection: &mut PgConnection,
    owner: &EnvironmentOwner,
    environment: veoveo_hub_sdk::EnvironmentId,
) -> Result<BTreeMap<veoveo_hub_sdk::ServerSlug, ComponentDefinition>> {
    let mut components = BTreeMap::new();
    let mut after = String::new();
    loop {
        let page = sqlx::query_file!(
            "src/bin/hub/activation/source_list.sql",
            owner.tenant.as_str(),
            owner.principal.as_str(),
            environment.as_uuid(),
            &after,
            ACTIVATION_PAGE_SIZE
        )
        .fetch_all(&mut *connection)
        .await?;
        if page.is_empty() {
            break;
        }
        for row in page {
            after = row.component;
            components.insert(
                veoveo_hub_sdk::ServerSlug::parse(&after)?,
                row.document.0.component,
            );
        }
    }
    Ok(components)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplyReceipt {
    definition: EnvironmentDefinition,
    accepted: Vec<AcceptedApply>,
}

#[cfg(test)]
#[path = "source/tests.rs"]
mod tests;
