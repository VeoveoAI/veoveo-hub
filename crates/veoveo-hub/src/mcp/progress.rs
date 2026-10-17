use std::{collections::HashMap, sync::Arc};

use rmcp::model::ProgressToken;
use tokio::sync::RwLock;
use veoveo_hub_sdk::{HubProfileId, ServerSlug};
use veoveo_types::PrincipalId;

#[derive(Debug, Clone, Default)]
pub(super) struct HubProgressTokens {
    inner: Arc<RwLock<HashMap<HubProgressTokenKey, HubProgressMapping>>>,
}

impl HubProgressTokens {
    pub(super) async fn register(
        &self,
        profile: &HubProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: ProgressToken,
        downstream_token: ProgressToken,
    ) {
        self.inner.write().await.insert(
            HubProgressTokenKey::new(profile, principal, upstream_server, upstream_token),
            HubProgressMapping { downstream_token },
        );
    }

    pub(super) async fn translate(
        &self,
        profile: &HubProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: &ProgressToken,
    ) -> Option<ProgressToken> {
        self.inner
            .read()
            .await
            .get(&HubProgressTokenKey::new(
                profile,
                principal,
                upstream_server,
                upstream_token.clone(),
            ))
            .map(|mapping| mapping.downstream_token.clone())
    }

    pub(super) async fn remove_token(
        &self,
        profile: &HubProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: &ProgressToken,
    ) {
        self.inner.write().await.remove(&HubProgressTokenKey::new(
            profile,
            principal,
            upstream_server,
            upstream_token.clone(),
        ));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HubProgressTokenKey {
    profile: HubProfileId,
    principal: PrincipalId,
    upstream_server: ServerSlug,
    upstream_token: ProgressToken,
}

impl HubProgressTokenKey {
    fn new(
        profile: &HubProfileId,
        principal: &PrincipalId,
        upstream_server: &ServerSlug,
        upstream_token: ProgressToken,
    ) -> Self {
        Self {
            profile: profile.clone(),
            principal: principal.clone(),
            upstream_server: upstream_server.clone(),
            upstream_token,
        }
    }
}

#[derive(Debug, Clone)]
struct HubProgressMapping {
    downstream_token: ProgressToken,
}

#[cfg(test)]
mod tests {
    use rmcp::model::NumberOrString;

    use super::*;

    fn token(value: i64) -> ProgressToken {
        ProgressToken(NumberOrString::Number(value))
    }

    #[tokio::test]
    async fn progress_tokens_translate_and_remove_by_token() {
        let registry = HubProgressTokens::default();
        let profile = HubProfileId::parse("default").unwrap();
        let principal = PrincipalId::parse("issuer#subject").unwrap();
        let server = ServerSlug::parse("upstream-a").unwrap();

        registry
            .register(&profile, &principal, &server, token(1), token(99))
            .await;
        assert_eq!(
            registry
                .translate(&profile, &principal, &server, &token(1))
                .await,
            Some(token(99))
        );

        registry
            .remove_token(&profile, &principal, &server, &token(1))
            .await;
        assert_eq!(
            registry
                .translate(&profile, &principal, &server, &token(1))
                .await,
            None
        );
    }
}
