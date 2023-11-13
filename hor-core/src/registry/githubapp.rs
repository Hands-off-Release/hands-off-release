use super::{Registry, SourceProjects};

#[derive(Default)]
pub struct GithubAppRegistry(SourceProjects);

impl Registry for GithubAppRegistry {
    fn get_projects(&self) -> &super::SourceProjects {
        &self.0
    }
}
