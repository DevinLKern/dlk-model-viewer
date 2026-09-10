use ash::vk;

mod depth_technique;
mod grid_technique;
mod main_technique;

pub use depth_technique::*;
pub use grid_technique::*;
pub use main_technique::*;

use crate::{FrameContext, Renderer, Result};

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
