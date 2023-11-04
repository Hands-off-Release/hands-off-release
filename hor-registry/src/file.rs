use derive_more::Constructor;

use crate::{Registry, SourceProjects};

#[derive(Constructor)]
pub struct FileBasedRegistry {
    source_projects: Box<SourceProjects>,
}

impl Registry for FileBasedRegistry {
    fn get_projects(&self) -> &SourceProjects {
        self.source_projects.as_ref()
    }
}
