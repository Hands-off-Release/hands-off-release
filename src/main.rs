use hor_core::{HorSystem, RefType};
use hor_registry::file::FileBasedRegistry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let registry = RefType::new(FileBasedRegistry::from_file("examples/example")?);
    HorSystem::new(registry, "local")?.init()?;
    Ok(())
}
