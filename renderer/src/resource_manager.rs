use std::{collections::HashMap, hash::Hash};

use ash::vk;
use vulkan::SharedDeviceRef;

use crate::{Error, Result};

// SHADER MODULES

#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone)]
pub enum ShaderModuleDescription {
    Internal {
        stage: vk::ShaderStageFlags,
        spv: &'static [u8],
        entry_point_name: &'static std::ffi::CStr,
        vertex_attribute_descriptions: *const [vk::VertexInputAttributeDescription],
        vertex_input_bindings: *const [vk::VertexInputBindingDescription],
    },
    External {
        stage: vk::ShaderStageFlags,
        path: Box<str>,
        entry_point_name: std::ffi::CString,
    },
}

impl ShaderModuleDescription {
    #[inline]
    pub fn entry_point_name(&self) -> *const i8 {
        match self {
            ShaderModuleDescription::Internal { entry_point_name, .. } => entry_point_name.as_ptr(),
            ShaderModuleDescription::External { entry_point_name, .. } => entry_point_name.as_ptr(),
        }
    }
    #[inline]
    pub fn vertex_input_attribues(&self) -> *const [vk::VertexInputAttributeDescription] {
        match self {
            ShaderModuleDescription::Internal { vertex_attribute_descriptions, .. } => *vertex_attribute_descriptions,
            _ => todo!()
        }
    }
    #[inline]
    pub fn vertex_input_bindings(&self) -> *const [vk::VertexInputBindingDescription] {
        match self {
            ShaderModuleDescription::Internal { vertex_input_bindings, .. } => *vertex_input_bindings,
            _ => todo!()
        }
    }
}

slotmap::new_key_type! { pub struct ShaderModuleResourceHandle; }

struct ShaderModuleResource {
    pub raw: vk::ShaderModule,
    pub desc: ShaderModuleDescription,
}

pub struct ShaderModuleResourceManager {
    device: vulkan::SharedDeviceRef,
    cache: HashMap<ShaderModuleDescription, ShaderModuleResourceHandle>,
    resources: slotmap::SlotMap<ShaderModuleResourceHandle, ShaderModuleResource>,
}

#[allow(unused)]
impl ShaderModuleResourceManager {
    pub(crate) fn new(device: vulkan::SharedDeviceRef) -> Self {
        Self {
            device,
            cache: HashMap::new(),
            resources: slotmap::SlotMap::with_key(),
        }
    }
    pub(crate) fn get_desc(
        &self,
        handle: ShaderModuleResourceHandle,
    ) -> Option<&ShaderModuleDescription> {
        let r = self.resources.get(handle)?;
        Some(&r.desc)
    }
    pub(crate) fn access_or_create(
        &mut self,
        desc: ShaderModuleDescription,
    ) -> Result<ShaderModuleResourceHandle> {
        if let Some(&handle) = self.cache.get(&desc) {
            return Ok(handle);
        }

        let val = match desc {
            ShaderModuleDescription::Internal { spv, .. } => {
                let create_info = vk::ShaderModuleCreateInfo {
                    code_size: spv.len(),
                    p_code: spv.as_ptr() as *const u32,
                    ..Default::default()
                };

                unsafe { self.device.create_shader_module(&create_info) }?
            }
            _ => todo!(),
        };

        let resource = ShaderModuleResource {
            raw: val,
            desc: desc.clone(),
        };

        let handle = self.resources.insert(resource);
        self.cache.insert(desc, handle);
        Ok(handle)
    }
    pub(crate) fn get(&self, handle: ShaderModuleResourceHandle) -> Option<&vk::ShaderModule> {
        let r = self.resources.get(handle)?;
        Some(&r.raw)
    }
    pub(crate) fn destroy(&mut self, handle: ShaderModuleResourceHandle) {
        if let Some(resource) = self.resources.remove(handle) {
            self.cache.remove(&resource.desc);

            unsafe { self.device.destroy_shader_module(resource.raw) };
        }
    }
}

impl Drop for ShaderModuleResourceManager {
    fn drop(&mut self) {
        for (_handle, resource) in self.resources.iter() {
            unsafe {
                self.device.destroy_shader_module(resource.raw);
            }
        }
    }
}

// DESCRIPTOR SET LAYOUT

#[derive(Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct DescriptorSetLayoutBindingInfo {
    pub(crate) binding: u32,
    pub(crate) ty: vk::DescriptorType,
    pub(crate) count: u32,
    pub(crate) stage_flags: vk::ShaderStageFlags,
}

#[derive(Clone, Hash, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct DescriptorSetLayoutDescription {
    pub(crate) bindings: Box<[DescriptorSetLayoutBindingInfo]>,
}

#[allow(unused)]
struct DescriptorSetLayoutResource {
    raw: vk::DescriptorSetLayout,
    desc: DescriptorSetLayoutDescription,
}

slotmap::new_key_type! { pub(crate) struct DescriptorSetLayoutResourceHandle; }

pub(crate) struct DescriptorSetLayoutResourceManager {
    device: vulkan::SharedDeviceRef,
    cache: HashMap<DescriptorSetLayoutDescription, DescriptorSetLayoutResourceHandle>,
    resources: slotmap::SlotMap<DescriptorSetLayoutResourceHandle, DescriptorSetLayoutResource>,
}

impl DescriptorSetLayoutResourceManager {
    pub(crate) fn new(device: SharedDeviceRef) -> Self {
        Self {
            device,
            cache: HashMap::new(),
            resources: slotmap::SlotMap::with_key(),
        }
    }
    pub(crate) fn access_or_create(
        &mut self,
        desc: DescriptorSetLayoutDescription,
    ) -> Result<DescriptorSetLayoutResourceHandle> {
        if let Some(&handle) = self.cache.get(&desc) {
            return Ok(handle);
        }

        let raw = {
            let bindings: Box<[vk::DescriptorSetLayoutBinding]> = desc
                .bindings
                .iter()
                .map(|b| vk::DescriptorSetLayoutBinding {
                    binding: b.binding,
                    descriptor_type: b.ty,
                    descriptor_count: b.count,
                    stage_flags: b.stage_flags,
                    ..Default::default()
                })
                .collect();
            let create_info = vk::DescriptorSetLayoutCreateInfo {
                binding_count: bindings.len() as u32,
                p_bindings: bindings.as_ptr(),
                ..Default::default()
            };
            unsafe { self.device.create_descriptor_set_layout(&create_info) }?
        };

        let handle = self.resources.insert(DescriptorSetLayoutResource {
            raw,
            desc: desc.clone(),
        });
        self.cache.insert(desc, handle);
        Ok(handle)
    }
    pub(crate) fn get(
        &self,
        handle: DescriptorSetLayoutResourceHandle,
    ) -> Option<&vk::DescriptorSetLayout> {
        let r = self.resources.get(handle)?;
        Some(&r.raw)
    }
    #[allow(unused)]
    pub(crate) fn destroy(&mut self, handle: DescriptorSetLayoutResourceHandle) {
        if let Some(resource) = self.resources.remove(handle) {
            unsafe { self.device.destroy_descriptor_set_layout(resource.raw) };
        }
    }
}

impl Drop for DescriptorSetLayoutResourceManager {
    fn drop(&mut self) {
        for (_handle, resource) in self.resources.iter() {
            unsafe {
                self.device.destroy_descriptor_set_layout(resource.raw);
            }
        }
    }
}

// PIPELINE LAYOUT

#[derive(Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct PipelineLayoutDescription {
    pub(crate) descriptor_set_layouts: Box<[DescriptorSetLayoutResourceHandle]>,
    pub bind_point: vk::PipelineBindPoint,
}

slotmap::new_key_type! { pub struct PipelineLayoutResourceHandle; }

pub(crate) struct PipelineLayoutResource {
    pub raw: vk::PipelineLayout,
    pub desc: PipelineLayoutDescription,
}

pub struct PipelineLayoutResourceManager {
    device: vulkan::SharedDeviceRef,
    cache: HashMap<PipelineLayoutDescription, PipelineLayoutResourceHandle>,
    resources: slotmap::SlotMap<PipelineLayoutResourceHandle, PipelineLayoutResource>,
}

impl PipelineLayoutResourceManager {
    pub(crate) fn new(device: SharedDeviceRef) -> Self {
        Self {
            device,
            cache: HashMap::new(),
            resources: slotmap::SlotMap::with_key(),
        }
    }
    pub(crate) fn access_or_create(
        &mut self,
        desc: PipelineLayoutDescription,
        descriptor_set_layouts: &mut DescriptorSetLayoutResourceManager,
    ) -> crate::Result<PipelineLayoutResourceHandle> {
        if let Some(&handle) = self.cache.get(&desc) {
            return Ok(handle);
        }

        let val = {
            let mut set_layouts_raw =
                Vec::<vk::DescriptorSetLayout>::with_capacity(desc.descriptor_set_layouts.len());
            for handle in desc.descriptor_set_layouts.iter() {
                let layout = descriptor_set_layouts.get(*handle).unwrap();
                set_layouts_raw.push(*layout);
            }
            let create_info = &vk::PipelineLayoutCreateInfo {
                set_layout_count: set_layouts_raw.len() as u32,
                p_set_layouts: set_layouts_raw.as_ptr(),
                ..Default::default()
            };
            unsafe { self.device.create_pipeline_layout(create_info) }?
        };

        let handle = self.resources.insert(PipelineLayoutResource {
            raw: val,
            desc: desc.clone(),
        });
        self.cache.insert(desc, handle);
        Ok(handle)
    }
    #[inline]
    pub(crate) fn get(
        &self,
        handle: PipelineLayoutResourceHandle,
    ) -> Option<&PipelineLayoutResource> {
        self.resources.get(handle)
    }
    pub(crate) fn destroy(&mut self, handle: PipelineLayoutResourceHandle) {
        if let Some(val) = self.resources.remove(handle) {
            unsafe { self.device.destroy_pipeline_layout(val.raw) };
        }
    }
}

impl Drop for PipelineLayoutResourceManager {
    fn drop(&mut self) {
        for (_handle, resource) in self.resources.iter() {
            unsafe {
                self.device.destroy_pipeline_layout(resource.raw);
            }
        }
    }
}

// PIPELINES

#[allow(unused)]
#[derive(Hash, Eq, PartialEq, PartialOrd, Ord, Clone)]
pub(crate) enum PipelineDescription {
    DynamicGraphics {
        pipeline_layout: PipelineLayoutResourceHandle,
        vert_shader: ShaderModuleResourceHandle,
        frag_shader: ShaderModuleResourceHandle,
        topology: vk::PrimitiveTopology,
        color_format: vk::Format,
        depth_format: vk::Format,
        samples: vk::SampleCountFlags,
    },
    ComputeInternal {
        pipeline_layout: PipelineLayoutResourceHandle,
        shader_module: ShaderModuleResourceHandle,
        entry_point: &'static std::ffi::CStr,
    }
}

slotmap::new_key_type! { pub struct PipelineResourceHandle; }

pub struct PipelineResource {
    raw: vk::Pipeline,
    desc: PipelineDescription,
}

pub struct PipelineResourceManager {
    device: vulkan::SharedDeviceRef,
    cache: HashMap<PipelineDescription, PipelineResourceHandle>,
    resources: slotmap::SlotMap<PipelineResourceHandle, PipelineResource>,
}

#[allow(unused)]
impl PipelineResourceManager {
    pub(crate) fn new(device: vulkan::SharedDeviceRef) -> Self {
        Self {
            device,
            cache: HashMap::new(),
            resources: slotmap::SlotMap::with_key(),
        }
    }
    pub(crate) fn access_or_create(
        &mut self,
        desc: PipelineDescription,
        pipeline_layouts: &mut PipelineLayoutResourceManager,
        shader_modules: &mut ShaderModuleResourceManager,
    ) -> crate::Result<PipelineResourceHandle> {
        if let Some(&handle) = self.cache.get(&desc) {
            return Ok(handle);
        }

        let raw = match &desc {
            PipelineDescription::DynamicGraphics {
                pipeline_layout,
                vert_shader,
                frag_shader,
                topology,
                color_format,
                depth_format,
                samples,
            } => {
                let (vert_stage, vert_input_attributes, vert_input_bindings) = {
                    let vert_shader_desc = shader_modules
                        .get_desc(*vert_shader)
                        .ok_or(Error::ResourceMissing)?;
                    let vert_shader = shader_modules
                        .get(*vert_shader)
                        .ok_or(Error::ResourceMissing)?;

                    let vert_stage = vk::PipelineShaderStageCreateInfo {
                        stage: vk::ShaderStageFlags::VERTEX,
                        module: *vert_shader,
                        p_name: vert_shader_desc.entry_point_name(),
                        ..Default::default()
                    };

                    (vert_stage, vert_shader_desc.vertex_input_attribues(), vert_shader_desc.vertex_input_bindings())
                };

                let (frag_stage, frag_input_attributes, frag_input_bindings) = {
                    let frag_shader_desc = shader_modules
                        .get_desc(*frag_shader)
                        .ok_or(Error::ResourceMissing)?;
                    let frag_shader = shader_modules
                        .get(*frag_shader)
                        .ok_or(Error::ResourceMissing)?;
                    let frag_stage = vk::PipelineShaderStageCreateInfo {
                        stage: vk::ShaderStageFlags::FRAGMENT,
                        module: *frag_shader,
                        p_name: frag_shader_desc.entry_point_name(),
                        ..Default::default()
                    };

                    (frag_stage, frag_shader_desc.vertex_input_attribues(), frag_shader_desc.vertex_input_bindings())
                };

                let stages = [vert_stage, frag_stage];

                let vertex_input_state = vk::PipelineVertexInputStateCreateInfo {
                    vertex_binding_description_count: vert_input_bindings.len() as u32,
                    p_vertex_binding_descriptions: vert_input_bindings.cast(),
                    vertex_attribute_description_count: vert_input_attributes.len() as u32,
                    p_vertex_attribute_descriptions: vert_input_attributes.cast(),
                    ..Default::default()
                };
                let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo {
                    topology: *topology,
                    primitive_restart_enable: vk::FALSE,
                    ..Default::default()
                };
                let viewport_state = vk::PipelineViewportStateCreateInfo {
                    viewport_count: 1,
                    p_viewports: std::ptr::null(), // Since dynamic viewports is enabled this can be null
                    scissor_count: 1,
                    p_scissors: std::ptr::null(), // this is also be dynamic
                    ..Default::default()
                };
                let rasterization_state = vk::PipelineRasterizationStateCreateInfo {
                    depth_clamp_enable: vk::FALSE,
                    rasterizer_discard_enable: vk::FALSE,
                    polygon_mode: vk::PolygonMode::FILL,
                    cull_mode: vk::CullModeFlags::NONE,
                    front_face: vk::FrontFace::CLOCKWISE,
                    depth_bias_enable: vk::FALSE,
                    depth_bias_constant_factor: 0.0,
                    depth_bias_clamp: 0.0,
                    depth_bias_slope_factor: 0.0,
                    line_width: 1.0, // dyamic states is on and VK_DYNAMIC_STATE_LINE_WIDTH is not
                    ..Default::default()
                };
                let multisample_state = vk::PipelineMultisampleStateCreateInfo {
                    rasterization_samples: *samples,
                    sample_shading_enable: vk::FALSE,
                    ..Default::default()
                };
                let depth_stencil_state = vk::PipelineDepthStencilStateCreateInfo {
                    depth_test_enable: vk::TRUE,
                    depth_write_enable: vk::TRUE,
                    depth_compare_op: vk::CompareOp::LESS,
                    depth_bounds_test_enable: vk::FALSE,
                    stencil_test_enable: vk::FALSE,
                    min_depth_bounds: 0.0,
                    max_depth_bounds: 1.0,
                    ..Default::default()
                };
                let attachments = [vk::PipelineColorBlendAttachmentState {
                    blend_enable: vk::TRUE,
                    src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
                    dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
                    color_blend_op: vk::BlendOp::ADD,
                    src_alpha_blend_factor: vk::BlendFactor::ONE,
                    dst_alpha_blend_factor: vk::BlendFactor::ZERO,
                    alpha_blend_op: vk::BlendOp::ADD,
                    color_write_mask: vk::ColorComponentFlags::RGBA,
                }];
                let color_blend_state = vk::PipelineColorBlendStateCreateInfo {
                    logic_op_enable: vk::FALSE,
                    logic_op: vk::LogicOp::COPY,
                    attachment_count: attachments.len() as u32,
                    p_attachments: attachments.as_ptr(),
                    blend_constants: [0.0, 0.0, 0.0, 0.0],
                    ..Default::default()
                };
                let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
                let dynamic_state = vk::PipelineDynamicStateCreateInfo {
                    dynamic_state_count: dynamic_states.len() as u32,
                    p_dynamic_states: dynamic_states.as_ptr(),
                    ..Default::default()
                };
                let color_formats = [*color_format];
                let pipeline_rendering_info = vk::PipelineRenderingCreateInfo {
                    color_attachment_count: color_formats.len() as u32,
                    p_color_attachment_formats: color_formats.as_ptr(),
                    depth_attachment_format: *depth_format,
                    stencil_attachment_format: *depth_format,
                    ..Default::default()
                };
                let pipeline_create_infos = [vk::GraphicsPipelineCreateInfo {
                    p_next: &pipeline_rendering_info as *const _ as *const std::ffi::c_void,
                    stage_count: stages.len() as u32,
                    p_stages: stages.as_ptr(),
                    p_vertex_input_state: &vertex_input_state,
                    p_input_assembly_state: &input_assembly_state,
                    p_tessellation_state: std::ptr::null(),
                    p_viewport_state: &viewport_state,
                    p_rasterization_state: &rasterization_state,
                    p_multisample_state: &multisample_state,
                    p_depth_stencil_state: &depth_stencil_state,
                    p_color_blend_state: &color_blend_state,
                    p_dynamic_state: &dynamic_state,
                    layout: pipeline_layouts.get(*pipeline_layout).unwrap().raw,
                    render_pass: vk::RenderPass::null(), // dynamic rendering is enabled
                    subpass: 0,
                    ..Default::default()
                }];

                let pipelines = unsafe {
                    self.device.create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &pipeline_create_infos,
                    )
                };
                let pipelines = pipelines.map_err(|(_, e)| e)?;

                pipelines[0]
            }
            PipelineDescription::ComputeInternal { pipeline_layout, shader_module, entry_point } => {
                let layout = pipeline_layouts.get(*pipeline_layout).ok_or(Error::ResourceMissing)?;
                let (layout, bind_point) = (layout.raw, layout.desc.bind_point);
                let module = *shader_modules.get(*shader_module).ok_or(Error::ResourceMissing)?;

                let stage = vk::PipelineShaderStageCreateInfo {
                    stage: vk::ShaderStageFlags::COMPUTE,
                    module,
                    p_name: entry_point.as_ptr(),
                    ..Default::default()
                };
                let create_infos = [vk::ComputePipelineCreateInfo {
                    stage,
                    layout,
                    ..Default::default()
                }];

                let pipelines = unsafe { self.device.create_compute_pipelines(vk::PipelineCache::null(), &create_infos) };
                let pipelines = pipelines.map_err(|(_, e)| e)?;
                
                pipelines[0]
            }
        };

        let handle = self.resources.insert(PipelineResource {
            raw,
            desc: desc.clone(),
        });
        self.cache.insert(desc, handle);
        Ok(handle)
    }
    pub(crate) fn get(&self, handle: PipelineResourceHandle) -> Option<&vk::Pipeline> {
        let r = self.resources.get(handle)?;
        Some(&r.raw)
    }
    pub(crate) fn destroy(&mut self, handle: PipelineResourceHandle) {
        if let Some(resource) = self.resources.remove(handle) {
            self.cache.remove(&resource.desc);

            unsafe {
                self.device.destroy_pipeline(resource.raw);
            }
        }
    }
}

impl Drop for PipelineResourceManager {
    fn drop(&mut self) {
        for (_handle, resource) in self.resources.iter() {
            unsafe {
                self.device.destroy_pipeline(resource.raw);
            }
        }
    }
}
