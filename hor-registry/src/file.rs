use config::{Config, ConfigError, File};
use derive_more::From;
use serde::Deserialize;
use thiserror::Error;

use crate::{Registry, SourceProjects};

pub struct FileBasedRegistry {
    source_projects: Box<SourceProjects>,
}

impl FileBasedRegistry {
    pub fn from_file(path: &'static str) -> Result<FileBasedRegistry, ConfigRsError> {
        let config: SourceProjectsWrapper = Config::builder()
            .add_source(File::with_name(path))
            .build()?
            .try_deserialize()?;

        Ok(FileBasedRegistry {
            source_projects: Box::new(config.projects),
        })
    }
}

#[derive(Deserialize)]
struct SourceProjectsWrapper {
    projects: SourceProjects,
}

#[derive(Error, Debug, From)]
#[error("unable to initialize registry from configuration")]
pub struct ConfigRsError(#[source] ConfigError);

impl Registry for FileBasedRegistry {
    fn get_projects(&self) -> &SourceProjects {
        self.source_projects.as_ref()
    }
}
