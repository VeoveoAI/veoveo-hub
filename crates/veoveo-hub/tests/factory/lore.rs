//! A fake Lore repository service over TLS, as the Hub binary reaches Lore: `create` makes
//! a repository whose creator is the bearer token's `identity`, answers an existing
//! repository of the same ID and name with its record, and refuses an ID with another
//! name or a name of another ID, as Lore's v1 `RepositoryCreate` does. It records each
//! creation's token claims. Every other method is unimplemented. The repositories and
//! tokens are synthetic.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use base64::Engine as _;
use lore_proto::lore::{
    model::v1::Repository,
    repository::v1::{
        RepositoryCreateRequest, RepositoryCreateResponse, RepositoryDeleteRequest,
        RepositoryDeleteResponse, RepositoryGetRequest, RepositoryGetResponse,
        RepositoryListRequest, RepositoryListResponse, RepositoryMetadataGetRequest,
        RepositoryMetadataGetResponse, RepositoryMetadataSetRequest, RepositoryMetadataSetResponse,
        repository_service_server::{RepositoryService, RepositoryServiceServer},
    },
};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status};

#[derive(Clone, Default)]
pub struct FakeLore {
    repositories: Arc<Mutex<BTreeMap<String, Repository>>>,
    tokens: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl FakeLore {
    /// Serve the fake over TLS on a loopback port, with a certificate for 127.0.0.1 that a
    /// new authority signs. Returns its `grpcs` endpoint and the authority's PEM.
    pub async fn serve() -> (Self, url::Url, String) {
        let authority_key = rcgen::KeyPair::generate().unwrap();
        let mut authority = rcgen::CertificateParams::new(Vec::new()).unwrap();
        authority.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        authority
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Fake Lore authority");
        let authority_certificate = authority.self_signed(&authority_key).unwrap();
        let issuer = rcgen::Issuer::new(authority, authority_key);
        let key = rcgen::KeyPair::generate().unwrap();
        let certificate = rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()])
            .unwrap()
            .signed_by(&key, &issuer)
            .unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let lore = Self::default();
        let tls = tonic::transport::ServerTlsConfig::new().identity(
            tonic::transport::Identity::from_pem(certificate.pem(), key.serialize_pem()),
        );
        tokio::spawn(
            tonic::transport::Server::builder()
                .tls_config(tls)
                .unwrap()
                .add_service(RepositoryServiceServer::new(lore.clone()))
                .serve_with_incoming(TcpListenerStream::new(listener)),
        );
        (
            lore,
            url::Url::parse(&format!("grpcs://{address}")).unwrap(),
            authority_certificate.pem(),
        )
    }

    /// A repository Lore holds already, created by `creator`.
    pub fn seed(&self, id: &str, name: &str, creator: &str) {
        self.repositories.lock().unwrap().insert(
            id.to_owned(),
            Repository {
                id: hex::decode(id).unwrap().into(),
                name: name.into(),
                default_branch_name: "main".into(),
                creator: creator.into(),
                ..Default::default()
            },
        );
    }

    /// Who Lore records as the creator of `id`.
    pub fn creator(&self, id: &str) -> Option<String> {
        self.repositories
            .lock()
            .unwrap()
            .get(id)
            .map(|repository| repository.creator.clone())
    }

    /// The claims of each creation's token, oldest first.
    pub fn tokens(&self) -> Vec<serde_json::Value> {
        self.tokens.lock().unwrap().clone()
    }
}

/// The claims of the request's bearer token, unverified, as a test reads them.
fn claims<T>(request: &Request<T>) -> Result<serde_json::Value, Status> {
    let bearer = request
        .metadata()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| Status::unauthenticated("no bearer token"))?;
    let payload = bearer
        .split('.')
        .nth(1)
        .ok_or_else(|| Status::unauthenticated("not a JWT"))?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| Status::unauthenticated("not a JWT"))?;
    serde_json::from_slice(&payload).map_err(|_| Status::unauthenticated("not a JWT"))
}

#[tonic::async_trait]
impl RepositoryService for FakeLore {
    async fn repository_create(
        &self,
        request: Request<RepositoryCreateRequest>,
    ) -> Result<Response<RepositoryCreateResponse>, Status> {
        let claims = claims(&request)?;
        let identity = claims["identity"]
            .as_str()
            .ok_or_else(|| Status::unauthenticated("the token names no identity"))?
            .to_owned();
        self.tokens.lock().unwrap().push(claims);
        let request = request.into_inner();
        let id = hex::encode(&request.id);
        let mut repositories = self.repositories.lock().unwrap();
        if let Some(existing) = repositories.get(&id) {
            return if existing.name == request.name {
                Ok(Response::new(RepositoryCreateResponse {
                    repository: Some(existing.clone()),
                }))
            } else {
                Err(Status::already_exists("the ID has another name"))
            };
        }
        if repositories.values().any(|r| r.name == request.name) {
            return Err(Status::already_exists("the name belongs to another ID"));
        }
        let repository = Repository {
            id: request.id,
            name: request.name,
            description: request.description,
            default_branch_id: request.default_branch_id,
            default_branch_name: request.default_branch_name,
            creator: request
                .creator
                .filter(|c| !c.is_empty())
                .unwrap_or(identity),
            ..Default::default()
        };
        repositories.insert(id, repository.clone());
        Ok(Response::new(RepositoryCreateResponse {
            repository: Some(repository),
        }))
    }
    async fn repository_delete(
        &self,
        _: Request<RepositoryDeleteRequest>,
    ) -> Result<Response<RepositoryDeleteResponse>, Status> {
        Err(Status::unimplemented("the fake Lore only creates"))
    }
    async fn repository_get(
        &self,
        _: Request<RepositoryGetRequest>,
    ) -> Result<Response<RepositoryGetResponse>, Status> {
        Err(Status::unimplemented("the fake Lore only creates"))
    }
    type RepositoryListStream = futures::stream::Empty<Result<RepositoryListResponse, Status>>;
    async fn repository_list(
        &self,
        _: Request<RepositoryListRequest>,
    ) -> Result<Response<Self::RepositoryListStream>, Status> {
        Err(Status::unimplemented("the fake Lore only creates"))
    }
    async fn repository_metadata_get(
        &self,
        _: Request<RepositoryMetadataGetRequest>,
    ) -> Result<Response<RepositoryMetadataGetResponse>, Status> {
        Err(Status::unimplemented("the fake Lore only creates"))
    }
    async fn repository_metadata_set(
        &self,
        _: Request<RepositoryMetadataSetRequest>,
    ) -> Result<Response<RepositoryMetadataSetResponse>, Status> {
        Err(Status::unimplemented("the fake Lore only creates"))
    }
}
