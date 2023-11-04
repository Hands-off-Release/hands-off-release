use anyhow::{bail, Context};
use async_trait::async_trait;
use hor_registry::{GithubProject, Registry, SourceProject};
use mediator::Mediate;
use mediator_tracing::TracingModule;
use octocrab::{
    models::repos::{Object, Ref},
    params::repos::Reference,
    GitHubError, Octocrab, OctocrabBuilder,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, info_span};

type RefType<T> = Box<T>;

pub struct UninitializedState {}

pub struct InitializedState {
    octo: Octocrab,
}

pub struct HorSystem<State> {
    registry: RefType<dyn Registry>,
    state: State,
}

impl HorSystem<UninitializedState> {
    pub fn new(registry: RefType<dyn Registry>) -> Self {
        Self {
            registry,
            state: UninitializedState {},
        }
    }
}

impl HorSystem<InitializedState> {
    pub async fn sync(&self) -> anyhow::Result<()> {
        let projects = self.registry.get_projects();
        for project in projects {
            match project {
                SourceProject::Github(project) => self.update_github(project).await?,
                other => bail!("Project type currently not supported {:?}", other),
            }
        }
        Ok(())
    }

    async fn update_github(&self, project: &GithubProject) -> anyhow::Result<()> {
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

        let tag_sha = match repo_handler.get_ref(&Reference::Tag(env.to_string())).await {
            Ok(tag) => match tag.object {
                Object::Tag { sha, url: _ } => Some(sha),
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

        let _result = match tag_sha {
            Some(tag_sha) => match tag_sha == tracked_branch_sha {
                true => {
                    info!("Deployment already in appropriate spot");
                    return Ok(());
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

        Ok(())
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
pub struct HorSystemConfiguration {
    github_personal_token: String,
}

impl Mediate<HorSystemConfiguration> for HorSystem<UninitializedState> {
    type Out = Result<HorSystem<InitializedState>, HorSystemInitializationError>;

    fn mediate(self, config: HorSystemConfiguration) -> Self::Out {
        TracingModule::default().init();
        Ok(HorSystem {
            registry: self.registry,
            state: InitializedState {
                octo: OctocrabBuilder::default()
                    .personal_token(config.github_personal_token)
                    .build()
                    .map_err(|err| HorSystemInitializationError::Octo(err))?,
            },
        })
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum HorSystemInitializationError {
    #[error("an error occurred while initializing Octocrab")]
    Octo(octocrab::Error),
}

#[cfg(test)]
mod tests {
    use hor_registry::{GithubProject, Registry, SourceProject, SourceProjects};
    use mediator::Mediate;

    use crate::{HorSystem, HorSystemConfiguration};

    #[tokio::test]
    async fn it_works() -> anyhow::Result<()> {
        struct MockRegistry {
            projects: SourceProjects,
        }
        impl Registry for MockRegistry {
            fn get_projects(&self) -> &SourceProjects {
                &self.projects
            }
        }

        let system = HorSystem::new(Box::new(MockRegistry {
            projects: vec![SourceProject::Github(GithubProject {
                owner: "Hands-off-Release".to_string(),
                repo: "hands-off-release".to_string(),
                env: "deployment".to_string(),
            })],
        }))
        .mediate(HorSystemConfiguration {
            github_personal_token: String::new(),
        })?;

        system.sync().await
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
