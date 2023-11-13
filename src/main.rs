use config::{Config, File};
use hor_core::{registry::githubapp::GithubAppRegistry, HorSystem, InitializedState, RefType};
use mediator_config::configrs::ConfigRsAdapter;

#[shuttle_runtime::main]
async fn main() -> shuttle_tower::ShuttleTower<HorSystem<InitializedState>> {
    let system = hor_system().map_err(|err| shuttle_runtime::Error::Custom(err))?;
    Ok(system.into())
}

fn hor_system() -> anyhow::Result<HorSystem<InitializedState>> {
    let registry = RefType::new(GithubAppRegistry::default());
    let system = HorSystem::new(
        registry,
        ConfigRsAdapter(
            Config::builder()
                .add_source(File::with_name("local").required(false))
                .build()?,
        ),
    )?
    .init()?;
    Ok(system)
}
