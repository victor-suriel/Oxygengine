extern crate oxygengine_core as core;

pub mod component;
pub mod nav_mesh_asset_protocol;
pub mod resource;
pub mod system;

pub mod prelude {
    pub use crate::{component::*, nav_mesh_asset_protocol::*, resource::*, system::*};
}

use crate::{
    component::NavAgent,
    resource::NavMeshesRes,
    system::{NavAgentMaintainSystem, SimpleNavDriverSystem},
};
use core::{app::AppBuilder, prefab::PrefabManager};

pub fn bundle_installer<'a, 'b>(builder: &mut AppBuilder<'a, 'b>) {
    builder.install_resource(NavMeshesRes::default());
    builder.install_system(NavAgentMaintainSystem::default(), "nav-agent-maintain", &[]);
    builder.install_system(
        SimpleNavDriverSystem,
        "simple-nav-driver",
        &["nav-agent-maintain"],
    );
}

pub fn prefabs_installer(prefabs: &mut PrefabManager) {
    prefabs.register_component_factory::<NavAgent>("NavAgent");
}
