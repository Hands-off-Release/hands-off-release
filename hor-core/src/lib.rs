pub mod registry;

use anyhow::{bail, Context};
use async_trait::async_trait;
use config::ConfigError;
use mediator::{ConfigParseErr, ConfigProvider, Mediate};
use octocrab::{
    models::repos::{Object, Ref},
    params::repos::Reference,
    GitHubError, Octocrab, OctocrabBuilder,
};
use registry::{GithubProject, Registry, SourceProject};
use serde::Deserialize;
use serde_json::json;
use std::{convert::Infallible, future::Future, pin::Pin, sync::Arc};
use tower::Service;
use tracing::{info, info_span};

pub type RefType<T> = Arc<T>;

pub struct UninitializedState<CP> {
    config_provider: CP,
}

#[derive(Clone)]
pub struct InitializedState {
    octo: Octocrab,
}

#[derive(Clone)]
pub struct HorSystem<State> {
    registry: RefType<dyn Registry + Send + Sync>,
    state: State,
}

impl<CP: ConfigProvider> HorSystem<UninitializedState<CP>> {
    pub fn new(
        registry: RefType<dyn Registry + Send + Sync>,
        config_provider: CP,
    ) -> Result<Self, HorSystemInitializationError> {
        Ok(Self {
            registry,
            state: UninitializedState { config_provider },
        })
    }

    pub fn init(self) -> Result<HorSystem<InitializedState>, HorSystemInitializationError> {
        // TracingModule::default().init();
        let config = self.state.config_provider.extract("hor");

        let config = match config {
            Ok(config) => Some(config),
            Err(err) => match err {
                ConfigParseErr::NoKey => None,
                err => return Err(HorSystemInitializationError::ConfigParse(err)),
            },
        };

        self.mediate(config)
    }
}

impl HorSystem<InitializedState> {
    pub async fn sync(&self) -> anyhow::Result<()> {
        let projects = self.registry.get_projects();
        for project in projects {
            match project {
                SourceProject::Github(project) => {
                    let git_ref = self.update_github(project).await?;
                }
                other => bail!("Project type currently not supported {:?}", other),
            }
        }
        Ok(())
    }

    async fn update_github(&self, project: &GithubProject) -> anyhow::Result<Ref> {
        let _span = info_span!("update Github project", ?project).entered();
        let owner = project.owner.as_str();
        let repo_path = project.repo.as_str();
        let env = project.env.as_str();
        let repo_handler = self.state.octo.repos(owner, repo_path);
        let repo = repo_handler.get().await?;
        let tracked_branch_sha = Self::sha_for_ref(match repo.default_branch {
            Some(main_branch) => {
                repo_handler
                    .get_ref(&Reference::Branch(main_branch))
                    .await?
            }
            None => bail!("project does not have main branch defined"),
        })?;

        let tag = match repo_handler.get_ref(&Reference::Tag(env.to_string())).await {
            Ok(tag) => match tag.object {
                Object::Tag { sha, .. } => Some(sha),
                _ => bail!("unexpected ref type"),
            },
            Err(err) => match &err {
                octocrab::Error::GitHub {
                    source: GitHubError { message, .. },
                    ..
                } => match message == "Not Found" {
                    true => None,
                    false => bail!(err),
                },
                _ => bail!(err),
            },
        };

        fn full_ref(env: &str) -> String {
            format!("refs/tags/{env}")
        }

        let result = match tag {
            Some(tag) => match tag == tracked_branch_sha {
                true => {
                    info!("Deployment already in appropriate spot");
                    return Ok(todo!());
                }
                // Update ref
                false => self
                    .state
                    .octo
                    .update_ref(
                        owner.to_string(),
                        repo_path.to_string(),
                        full_ref(env),
                        tracked_branch_sha,
                    )
                    .await
                    .context("Unable to update existing ref"),
            },
            // Create ref
            None => self
                .state
                .octo
                .post::<_, Ref>(
                    format!("/repos/{}/{}/git/refs", owner, repo_path),
                    Some(&json!({
                        "ref": full_ref(env),
                        "sha": tracked_branch_sha,
                        "force": true
                    })),
                )
                .await
                .context("Unable to create new ref"),
        }?;

        Ok(result)
    }

    fn sha_for_ref(git_ref: Ref) -> anyhow::Result<String> {
        match git_ref.object {
            Object::Commit { sha, url: _ } => Ok(sha),
            Object::Tag { sha, url: _ } => Ok(sha),
            _ => bail!("Unexpected ref object type"),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct HorSystemConfiguration {
    github_personal_token: String,
}

impl<CP: ConfigProvider> Mediate<Option<HorSystemConfiguration>>
    for HorSystem<UninitializedState<CP>>
{
    type Out = Result<HorSystem<InitializedState>, HorSystemInitializationError>;

    fn mediate(self, config: Option<HorSystemConfiguration>) -> Self::Out {
        let mut octo = OctocrabBuilder::default();

        if let Some(config) = config {
            octo = octo.personal_token(config.github_personal_token);
        }

        let octo = octo.build()?;

        Ok(HorSystem {
            registry: self.registry,
            state: InitializedState { octo },
        })
    }
}

impl Service<hyper::Request<hyper::Body>> for HorSystem<InitializedState> {
    type Response = hyper::Response<hyper::Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + Sync>>;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: hyper::Request<hyper::Body>) -> Self::Future {
        info!("This works");
        let resp = hyper::Response::builder()
            .status(204)
            .body(hyper::Body::default())
            .expect("Unable to create the `hyper::Response` object");

        let fut = async { Ok(resp) };

        Box::pin(fut)
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum HorSystemInitializationError {
    #[error("config.rs error")]
    ConfigRs(#[source] ConfigError),
    #[error("unable to parse configuration")]
    ConfigParse(#[source] ConfigParseErr),
    #[error("an error occurred while initializing Octocrab")]
    Octo(#[source] octocrab::Error),
}

impl From<octocrab::Error> for HorSystemInitializationError {
    fn from(err: octocrab::Error) -> Self {
        HorSystemInitializationError::Octo(err)
    }
}

#[async_trait]
trait HorOctocrabExtension {
    async fn update_ref(
        &self,
        owner: String,
        repo: String,
        reference: String,
        sha: String,
    ) -> octocrab::Result<Ref>;
}

#[async_trait]
impl HorOctocrabExtension for Octocrab {
    async fn update_ref(
        &self,
        owner: String,
        repo: String,
        reference: String,
        sha: String,
    ) -> octocrab::Result<Ref> {
        self.patch::<Ref, _, _>(
            format!("/repos/{}/{}/git/refs/{}", owner, repo, reference),
            Some(&json!({
                "sha": sha,
                "force": true
            })),
        )
        .await
    }
}
