use ash::vk;

mod depth_technique;
mod grid_technique;
mod main_technique;

pub use depth_technique::*;
pub use grid_technique::*;
pub use main_technique::*;

use crate::{FrameContext, Renderer, Result};

pub trait HasBindingValue<T> {
    type Value;
    fn get(&self) -> Self::Value;
}

pub trait ShaderBinding<T> {
    const SET: u32;
    const BINDING: u32;
}

pub trait RenderTechnique {
    type TechniqueResources;
    fn bind(
        &self,
        cmd: vk::CommandBuffer,
        ctx: &FrameContext,
        resources: &Self::TechniqueResources,
        renderer: &Renderer,
    ) -> Result<()>;
}
