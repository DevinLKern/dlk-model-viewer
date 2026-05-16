use crate::SharedInstanceRef;
use crate::result::{Error, Result};

use ash::prelude::VkResult;
use ash::vk;
use ash::vk::*;

// #[derive(Debug)]
pub struct Device {
    instance: SharedInstanceRef,
    physical_device: vk::PhysicalDevice,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    device: ash::Device,
    swapchain_loader: ash::khr::swapchain::Device,
    pub queue: vk::Queue, // TODO: rework queues
    queue_family_index: u32,
}

pub type SharedDeviceRef = std::sync::Arc<Device>;

macro_rules! vk_delegate_create {
    ($fn:ident, $info_ty:ident, $ret:ident) => {
        #[inline]
        pub unsafe fn $fn(&self, info: &vk::$info_ty) -> VkResult<vk::$ret> {
            unsafe { self.device.$fn(info, self.get_alloc_callbacks()) }
        }
    };
}

macro_rules! vk_delegate_destroy {
    ($fn:ident, $handle:ident) => {
        #[inline]
        pub unsafe fn $fn(&self, handle: vk::$handle) {
            unsafe { self.device.$fn(handle, self.get_alloc_callbacks()) }
        }
    };
}

macro_rules! vk_delegate_create_many {
    ($fn:ident, $info_ty:ident, $ret:ident) => {
        #[inline]
        pub unsafe fn $fn(&self, info: &vk::$info_ty) -> VkResult<Vec<vk::$ret>> {
            unsafe { self.device.$fn(info) }
        }
    };
}

macro_rules! vk_delegate_destroy_many {
    ($fn:ident, $pool_ty:ident, $handle_ty:ident) => {
        #[inline]
        pub unsafe fn $fn(&self, pool: vk::$pool_ty, handles: &[vk::$handle_ty]) {
            unsafe { self.device.$fn(pool, handles) }
        }
    };
}

macro_rules! vk_delegate_forward {
    ($fn:ident, ($($arg:ident : $ty:ty),*), $ret:ty) => {
        #[inline]
        pub unsafe fn $fn(&self, $($arg: $ty),*) -> $ret {
            unsafe { self.device.$fn($($arg),*) }
        }
    };
}

pub type SharedRef<T> = std::sync::Arc<T>;

#[allow(dead_code)]
impl Device {
    pub fn new(
        instance: SharedInstanceRef,
        pfn_debug_utils_callback: vk::PFN_vkDebugUtilsMessengerCallbackEXT,
    ) -> Result<SharedRef<Device>> {
        let debug_messenger = instance.create_debug_utils_messenger(pfn_debug_utils_callback)?;

        let queue_priority: f32 = 1.0;

        let (queue_create_info, physical_device) = {
            let all_physical_devices = unsafe { instance.raw().enumerate_physical_devices() }?;

            let viable_physical_devices: Box<[(usize, vk::PhysicalDevice)]> = all_physical_devices
                .into_iter()
                .enumerate()
                .filter(|(_, pd)| {
                    let mut properties = vk::PhysicalDeviceProperties2::default();
                    unsafe {
                        instance
                            .raw()
                            .get_physical_device_properties2(*pd, &mut properties);
                    }

                    if properties.properties.api_version < vk::API_VERSION_1_3 {
                        return false;
                    }

                    let queue_family_properties = unsafe {
                        let count = instance
                            .raw()
                            .get_physical_device_queue_family_properties2_len(*pd);
                        let mut properties =
                            vec![vk::QueueFamilyProperties2::default(); count].into_boxed_slice();
                        instance
                            .raw()
                            .get_physical_device_queue_family_properties2(*pd, properties.as_mut());
                        properties
                    };

                    if queue_family_properties
                        .iter()
                        .find(|qfp| {
                            qfp.queue_family_properties
                                .queue_flags
                                .contains(vk::QueueFlags::GRAPHICS)
                        })
                        .is_none()
                    {
                        return false;
                    }

                    true
                })
                .collect();

            if viable_physical_devices.len() == 0 {
                if let Some(messenger) = debug_messenger {
                    unsafe {
                        instance.destroy_debug_utils_messenger(messenger);
                    }
                }
                return Err(Error::NoViablePhysicalDevices);
            }

            match viable_physical_devices.into_iter().max_by_key(|(_, pd)| {
                let mut properties = vk::PhysicalDeviceProperties2::default();
                unsafe {
                    instance
                        .raw()
                        .get_physical_device_properties2(*pd, &mut properties);
                }

                match properties.properties.device_type {
                    vk::PhysicalDeviceType::CPU => 1,
                    vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
                    vk::PhysicalDeviceType::INTEGRATED_GPU => 3,
                    vk::PhysicalDeviceType::DISCRETE_GPU => 4,
                    _ => 0,
                }
            }) {
                Some((qfi, pd)) => (
                    vk::DeviceQueueCreateInfo {
                        queue_family_index: qfi.clone() as u32,
                        queue_count: 1,
                        p_queue_priorities: &queue_priority,
                        ..Default::default()
                    },
                    pd,
                ),
                None => {
                    unsafe {
                        if let Some(messenger) = debug_messenger {
                            instance.destroy_debug_utils_messenger(messenger);
                        }
                        instance
                            .raw()
                            .destroy_instance(instance.allocation_callbacks_ref());
                    }
                    return Err(Error::NoViablePhysicalDevices);
                }
            }
        };

        let device = {
            let enabled_device_extension_names = vec![ash::khr::swapchain::NAME.as_ptr()];

            let enabled_features = vk::PhysicalDeviceFeatures {
                sampler_anisotropy: vk::TRUE,
                ..Default::default()
            };
            let enabled_descriptor_indexing_features =
                vk::PhysicalDeviceDescriptorIndexingFeatures {
                    runtime_descriptor_array: vk::TRUE,
                    shader_sampled_image_array_non_uniform_indexing: vk::TRUE,
                    ..Default::default()
                };
            let synchronization2_features = vk::PhysicalDeviceSynchronization2Features {
                p_next: &enabled_descriptor_indexing_features as *const _ as *mut std::ffi::c_void,
                synchronization2: vk::TRUE,
                ..Default::default()
            };
            let dynamic_rendering_features = vk::PhysicalDeviceDynamicRenderingFeatures {
                p_next: &synchronization2_features as *const _ as *mut std::ffi::c_void,
                dynamic_rendering: vk::TRUE,
                ..Default::default()
            };
            let device_create_info = vk::DeviceCreateInfo {
                p_next: &dynamic_rendering_features as *const _ as *const std::ffi::c_void,
                queue_create_info_count: 1,
                p_queue_create_infos: &queue_create_info,
                enabled_extension_count: enabled_device_extension_names.len() as u32,
                pp_enabled_extension_names: enabled_device_extension_names.as_ptr(),
                p_enabled_features: &enabled_features,
                ..Default::default()
            };

            unsafe {
                instance
                    .raw()
                    .create_device(
                        physical_device,
                        &device_create_info,
                        instance.allocation_callbacks_ref(),
                    )
                    .inspect_err(|_| {
                        if let Some(messenger) = debug_messenger {
                            instance.destroy_debug_utils_messenger(messenger);
                        }
                    })?
            }
        };

        let swapchain_loader = ash::khr::swapchain::Device::new(instance.raw(), &device);

        let queue = {
            let get_queue_info = vk::DeviceQueueInfo2 {
                queue_family_index: queue_create_info.queue_family_index,
                queue_index: 0,
                ..Default::default()
            };
            unsafe { device.get_device_queue2(&get_queue_info) }
        };

        Ok(Device {
            instance,
            debug_messenger,
            physical_device,
            device,
            swapchain_loader,
            queue,
            queue_family_index: queue_create_info.queue_family_index,
        }
        .into())
    }

    #[inline]
    unsafe fn get_alloc_callbacks(&self) -> Option<&vk::AllocationCallbacks<'_>> {
        self.instance.allocation_callbacks_ref()
    }

    #[inline]
    pub unsafe fn get_physical_device_format_properties(
        &self,
        format: vk::Format,
    ) -> vk::FormatProperties {
        unsafe {
            self.instance
                .raw()
                .get_physical_device_format_properties(self.physical_device, format)
        }
    }

    #[inline]
    pub unsafe fn get_physical_device_properties(&self) -> vk::PhysicalDeviceProperties {
        unsafe {
            self.instance
                .raw()
                .get_physical_device_properties(self.physical_device)
        }
    }

    #[inline]
    pub unsafe fn get_physical_device_surface_formats(
        &self,
        surface: vk::SurfaceKHR,
    ) -> VkResult<Vec<vk::SurfaceFormatKHR>> {
        unsafe {
            self.instance
                .surface_loader
                .get_physical_device_surface_formats(self.physical_device, surface)
        }
    }

    #[inline]
    pub unsafe fn get_physical_device_surface_capabilities(
        &self,
        surface: vk::SurfaceKHR,
    ) -> VkResult<vk::SurfaceCapabilitiesKHR> {
        unsafe {
            self.instance
                .surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, surface)
        }
    }

    #[inline]
    pub unsafe fn get_physical_device_surface_present_modes(
        &self,
        surface: vk::SurfaceKHR,
    ) -> VkResult<Vec<vk::PresentModeKHR>> {
        unsafe {
            self.instance
                .surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, surface)
        }
    }
    #[inline]
    pub(crate) unsafe fn get_physical_device_memory_properties(
        &self,
    ) -> vk::PhysicalDeviceMemoryProperties {
        unsafe {
            self.instance
                .raw()
                .get_physical_device_memory_properties(self.physical_device)
        }
    }

    #[inline]
    pub unsafe fn create_graphics_pipelines(
        &self,
        pipeline_cache: vk::PipelineCache,
        create_infos: &[vk::GraphicsPipelineCreateInfo],
    ) -> std::result::Result<Vec<vk::Pipeline>, (Vec<vk::Pipeline>, vk::Result)> {
        unsafe {
            self.device.create_graphics_pipelines(
                pipeline_cache,
                create_infos,
                self.get_alloc_callbacks(),
            )
        }
    }

    #[inline]
    pub unsafe fn create_swapchain(
        &self,
        create_info: &vk::SwapchainCreateInfoKHR,
    ) -> VkResult<vk::SwapchainKHR> {
        unsafe {
            self.swapchain_loader
                .create_swapchain(create_info, self.get_alloc_callbacks())
        }
    }

    #[inline]
    pub unsafe fn destroy_swapchain(&self, swapchain: vk::SwapchainKHR) {
        unsafe {
            self.swapchain_loader
                .destroy_swapchain(swapchain, self.get_alloc_callbacks())
        }
    }

    #[inline]
    pub unsafe fn get_swapchain_images(
        &self,
        swapchain: vk::SwapchainKHR,
    ) -> VkResult<Vec<vk::Image>> {
        unsafe { self.swapchain_loader.get_swapchain_images(swapchain) }
    }

    #[inline]
    pub fn get_queue_family_index(&self) -> u32 {
        self.queue_family_index
    }

    #[inline]
    pub fn find_viable_depth_stencil_format(&self) -> Option<vk::Format> {
        let formats = [
            ash::vk::Format::D32_SFLOAT_S8_UINT,
            ash::vk::Format::D24_UNORM_S8_UINT,
            ash::vk::Format::D16_UNORM_S8_UINT,
        ];

        formats
            .into_iter()
            .filter_map(|f| {
                let properties = unsafe { self.get_physical_device_format_properties(f) };

                if properties
                    .optimal_tiling_features
                    .contains(ash::vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
                {
                    Some(f)
                } else {
                    None
                }
            })
            .next()
    }

    #[inline]
    pub unsafe fn create_surface(
        &self,
        window: &winit::window::Window,
    ) -> Result<ash::vk::SurfaceKHR> {
        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

        let display_handle = window.display_handle()?;
        let window_handle = window.window_handle()?;

        let surface = unsafe {
            ash_window::create_surface(
                &self.instance.entry,
                &self.instance.raw(),
                display_handle.as_raw(),
                window_handle.as_raw(),
                self.get_alloc_callbacks(),
            )
        }?;

        Ok(surface)
    }

    #[inline]
    pub unsafe fn destroy_surface(&self, surface: vk::SurfaceKHR) {
        unsafe {
            self.instance
                .surface_loader
                .destroy_surface(surface, self.get_alloc_callbacks())
        }
    }

    #[inline]
    pub(crate) unsafe fn acquire_next_image(
        &self,
        swapchain: vk::SwapchainKHR,
        semaphore: vk::Semaphore,
        fence: vk::Fence,
    ) -> VkResult<(u32, bool)> {
        unsafe {
            self.swapchain_loader
                .acquire_next_image(swapchain, u64::MAX, semaphore, fence)
        }
    }

    #[inline]
    pub unsafe fn queue_present(&self, present_info: &vk::PresentInfoKHR) -> VkResult<bool> {
        unsafe {
            self.swapchain_loader
                .queue_present(self.queue, present_info)
        }
    }

    vk_delegate_create!(allocate_memory, MemoryAllocateInfo, DeviceMemory);
    vk_delegate_destroy!(free_memory, DeviceMemory);
    vk_delegate_create!(create_buffer, BufferCreateInfo, Buffer);
    vk_delegate_destroy!(destroy_buffer, Buffer);
    vk_delegate_create!(create_image, ImageCreateInfo, Image);
    vk_delegate_destroy!(destroy_image, Image);
    vk_delegate_create!(create_image_view, ImageViewCreateInfo, ImageView);
    vk_delegate_destroy!(destroy_image_view, ImageView);
    vk_delegate_create!(create_shader_module, ShaderModuleCreateInfo, ShaderModule);
    vk_delegate_destroy!(destroy_shader_module, ShaderModule);
    vk_delegate_create!(
        create_pipeline_layout,
        PipelineLayoutCreateInfo,
        PipelineLayout
    );
    vk_delegate_destroy!(destroy_pipeline_layout, PipelineLayout);
    vk_delegate_create!(
        create_descriptor_set_layout,
        DescriptorSetLayoutCreateInfo,
        DescriptorSetLayout
    );
    vk_delegate_destroy!(destroy_descriptor_set_layout, DescriptorSetLayout);
    vk_delegate_destroy!(destroy_pipeline, Pipeline);
    vk_delegate_create!(create_command_pool, CommandPoolCreateInfo, CommandPool);
    vk_delegate_destroy!(destroy_command_pool, CommandPool);
    vk_delegate_create!(create_fence, FenceCreateInfo, Fence);
    vk_delegate_destroy!(destroy_fence, Fence);
    vk_delegate_create!(
        create_descriptor_pool,
        DescriptorPoolCreateInfo,
        DescriptorPool
    );
    vk_delegate_destroy!(destroy_descriptor_pool, DescriptorPool);
    vk_delegate_create!(create_semaphore, SemaphoreCreateInfo, Semaphore);
    vk_delegate_destroy!(destroy_semaphore, Semaphore);
    vk_delegate_create!(create_sampler, SamplerCreateInfo, Sampler);
    vk_delegate_destroy!(destroy_sampler, Sampler);
    vk_delegate_create_many!(
        allocate_command_buffers,
        CommandBufferAllocateInfo,
        CommandBuffer
    );
    vk_delegate_destroy_many!(free_command_buffers, CommandPool, CommandBuffer);

    vk_delegate_forward!(update_descriptor_sets, (writes: &[WriteDescriptorSet], copies: &[CopyDescriptorSet]), ());
    vk_delegate_forward!(cmd_copy_buffer2, (buffer: CommandBuffer, info: &CopyBufferInfo2), ());
    vk_delegate_forward!(cmd_copy_buffer_to_image2, (buffer: CommandBuffer, info: &CopyBufferToImageInfo2), ());
    vk_delegate_forward!(reset_fences, (fences: &[Fence]), VkResult<()>);
    vk_delegate_forward!(reset_command_buffer, (buffer: CommandBuffer, flags: CommandBufferResetFlags), VkResult<()>);
    vk_delegate_forward!(cmd_pipeline_barrier2, (cb: CommandBuffer, info: &DependencyInfo), ());
    vk_delegate_forward!(device_wait_idle, (), VkResult<()>);
    vk_delegate_forward!(cmd_bind_pipeline, (cb: CommandBuffer, bind_point: PipelineBindPoint, pipeline: Pipeline), ());
    vk_delegate_forward!(cmd_set_viewport, (buffer: CommandBuffer, first_viewport: u32, viewports: &[Viewport]), ());
    vk_delegate_forward!(cmd_set_scissor, (buffer: CommandBuffer, first_scissor: u32, scissors: &[Rect2D]), ());
    vk_delegate_forward!(cmd_bind_vertex_buffers, (command_buffer: CommandBuffer, first_binding: u32, buffers: &[Buffer], offsets: &[DeviceSize]), ());
    vk_delegate_forward!(cmd_bind_index_buffer, (command_buffer: CommandBuffer, buffer: Buffer, offset: DeviceSize, index_type: IndexType), ());
    vk_delegate_forward!(allocate_descriptor_sets, (info: &DescriptorSetAllocateInfo), VkResult<Vec<DescriptorSet>>);
    vk_delegate_forward!(free_descriptor_sets, (pool: DescriptorPool, sets: &[DescriptorSet]), VkResult<()>);
    vk_delegate_forward!(begin_command_buffer, (buffer: CommandBuffer,  info: &CommandBufferBeginInfo), VkResult<()>);
    vk_delegate_forward!(end_command_buffer, (buffer: CommandBuffer), VkResult<()>);
    vk_delegate_forward!(cmd_begin_rendering, (buffer: CommandBuffer, info: &RenderingInfo), ());
    vk_delegate_forward!(cmd_end_rendering, (buffer: CommandBuffer), ());
    vk_delegate_forward!(wait_for_fences, (fences: &[Fence], wait_all: bool, timeout: u64), VkResult<()>);
    vk_delegate_forward!(queue_submit, (queue: Queue, submits: &[SubmitInfo], fence: Fence), VkResult<()>);
    vk_delegate_forward!(bind_image_memory, (image: Image, memory: DeviceMemory, offset: DeviceSize), VkResult<()>);
    vk_delegate_forward!(bind_buffer_memory, (buffer: Buffer, memory: DeviceMemory, offset: DeviceSize), VkResult<()>);
    vk_delegate_forward!(get_buffer_memory_requirements, (buffer: Buffer), MemoryRequirements);
    vk_delegate_forward!(get_image_memory_requirements, (image: Image), MemoryRequirements);
    vk_delegate_forward!(map_memory, (memory: DeviceMemory, offset: DeviceSize, size: DeviceSize, flags: MemoryMapFlags), VkResult<*mut std::ffi::c_void>);
    vk_delegate_forward!(unmap_memory, (memory: DeviceMemory), ());
    vk_delegate_forward!(cmd_bind_descriptor_sets,(buffer: CommandBuffer, bind_point: PipelineBindPoint, layout: PipelineLayout, first_set: u32, sets: &[DescriptorSet], dynamic_offsets: &[u32]), ());
    vk_delegate_forward!(cmd_draw_indirect, (cmd: CommandBuffer, buffer: Buffer, offset: u64, draw_count: u32, stride: u32), ());
    vk_delegate_forward!(cmd_draw_indexed_indirect, (cmd: CommandBuffer, buffer: Buffer, offset: u64, draw_count: u32, stride: u32), ());
    vk_delegate_forward!(cmd_draw_indexed, (cmd: CommandBuffer, index_count: u32, instance_count: u32, first_index: u32, vertex_offset: i32, first_instance: u32), ());
    vk_delegate_forward!(cmd_draw, (cmd: vk::CommandBuffer, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32), ());
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(self.get_alloc_callbacks());
            if let Some(messenger) = self.debug_messenger {
                self.instance.destroy_debug_utils_messenger(messenger);
            }
        }
    }
}
