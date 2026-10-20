use super::*;

#[derive(Clone)]
struct EndingCatalog(std::sync::Arc<tokio::sync::Notify>);

impl rmcp::ServerHandler for EndingCatalog {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_resources()
                .enable_resources_list_changed()
                .build(),
        )
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: rmcp::service::SubscriptionContext) -> Result<(), McpError> {
        tokio::select! { _ = self.0.notified() => {}, _ = context.cancelled() => {} }
        Ok(())
    }
}

#[tokio::test]
async fn an_ended_catalog_source_reaches_the_downstream_reconnect_path() {
    use rmcp::{ClientServiceExt, ServiceExt};
    let end = std::sync::Arc::new(tokio::sync::Notify::new());
    let handler = EndingCatalog(end.clone());
    let (server_io, client_io) = tokio::io::duplex(8192);
    let server = tokio::spawn(async move { handler.serve(server_io).await.unwrap() });
    let mut client = ()
        .serve_with_lifecycle(
            client_io,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    let mut server = server.await.unwrap();
    let subscription = client
        .listen(
            SubscriptionFilter::builder()
                .resources_list_changed()
                .build(),
        )
        .await
        .unwrap();
    let mut routed = Box::pin(routed_notifications(
        (
            ServerSlug::parse("catalog").unwrap(),
            DeploymentId::new(),
            veoveo_hub_sdk::ResolvedComposition::new(
                veoveo_hub_sdk::EnvironmentId::new(),
                Default::default(),
            )
            .revision()
            .clone(),
        ),
        false,
        subscription,
    ));
    end.notify_one();
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), routed.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event.2, Ok(None)));
    let _ = client
        .close_with_timeout(std::time::Duration::from_secs(2))
        .await;
    let _ = server
        .close_with_timeout(std::time::Duration::from_secs(2))
        .await;
}
