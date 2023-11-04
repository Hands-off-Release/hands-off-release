pub mod file;

use serde::{Deserialize, Serialize};

pub trait Registry {
    fn get_projects(&self) -> &SourceProjects;
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum SourceProject {
    Github(GithubProject),
}

pub type SourceProjects = Vec<SourceProject>;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct GithubProject {
    pub owner: String,
    pub repo: String,
    pub env: String,
}
