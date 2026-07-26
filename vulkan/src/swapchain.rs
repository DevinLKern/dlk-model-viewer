use crate::device::SharedDeviceRef;
use crate::result::{Error, Result};
use ash::vk;

pub struct Swapchain {
    device: SharedDeviceRef,
    surface: vk::SurfaceKHR,
    swapchain: vk::SwapchainKHR,
    extent: vk::Extent2D,
    format: vk::Format,
    present_mode: vk::PresentModeKHR,
}

impl Swapchain {
    pub fn new(device: SharedDeviceRef, window: &winit::window::Window) -> Result<Swapchain> {
        let surface = unsafe { device.create_surface(window) }?;

        let surface_format = unsafe { device.get_physical_device_surface_formats(surface) }?
            .into_iter()
            .next()
            .ok_or(Error::NoSurfaceFomratsSupported)?;

        let (min_image_count, max_image_count, image_extent) = {
            let capabilities = unsafe { device.get_physical_device_surface_capabilities(surface) }?;

            let extent = if capabilities.current_extent.width == u32::MAX {
                ash::vk::Extent2D {
                    width: window.inner_size().width,
                    height: window.inner_size().height,
                }
            } else {
                capabilities.current_extent
            };

            if capabilities.min_image_count > capabilities.max_image_count {
                (
                    capabilities.min_image_count,
                    capabilities.min_image_count,
                    extent,
                )
            } else {
                (
                    capabilities.min_image_count,
                    capabilities.max_image_count,
                    extent,
                )
            }
        };

        let (present_mode, desired_image_count) = {
            let modes = unsafe { device.get_physical_device_surface_present_modes(surface) }?;

            if modes.contains(&ash::vk::PresentModeKHR::MAILBOX) {
                (ash::vk::PresentModeKHR::MAILBOX, 3)
            } else {
                (ash::vk::PresentModeKHR::FIFO, 2)
            }
        };

        let swapchain = {
            let swapchain_create_info = ash::vk::SwapchainCreateInfoKHR {
                surface: surface,
                min_image_count: desired_image_count.clamp(min_image_count, max_image_count),
                image_format: surface_format.format,
                image_color_space: surface_format.color_space,
                image_extent,
                image_usage: ash::vk::ImageUsageFlags::COLOR_ATTACHMENT,
                image_sharing_mode: ash::vk::SharingMode::EXCLUSIVE,
                present_mode,
                composite_alpha: ash::vk::CompositeAlphaFlagsKHR::OPAQUE,
                pre_transform: ash::vk::SurfaceTransformFlagsKHR::IDENTITY,
                clipped: ash::vk::FALSE,
                image_array_layers: 1,
                ..Default::default()
            };

            unsafe { device.create_swapchain(&swapchain_create_info) }?
        };

        Ok(Swapchain {
            device,
            surface,
            swapchain,
            format: surface_format.format,
            extent: image_extent,
            present_mode,
        })
    }

    #[inline]
    pub fn extent(&self) -> &vk::Extent2D {
        &self.extent
    }
    #[inline]
    pub fn surface(&self) -> vk::SurfaceKHR {
        self.surface
    }
    #[inline]
    pub fn format(&self) -> vk::Format {
        self.format
    }
    #[inline]
    pub unsafe fn get_images(&self) -> Result<Vec<vk::Image>> {
        let images = unsafe { self.device.get_swapchain_images(self.swapchain) }?;
        Ok(images)
    }
    pub fn present_mode(&self) -> vk::PresentModeKHR {
        self.present_mode
    }

    pub unsafe fn acquire_next_image(
        &self,
        semaphore: vk::Semaphore,
        fence: vk::Fence,
    ) -> ash::prelude::VkResult<(u32, bool)> {
        unsafe {
            self.device
                .acquire_next_image(self.swapchain, semaphore, fence)
        }
    }

    #[inline]
    pub unsafe fn get_swapchain_ptr(&self) -> *const vk::SwapchainKHR {
        &self.swapchain
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_swapchain(self.swapchain);

            self.device.destroy_surface(self.surface);
        }
    }
}
