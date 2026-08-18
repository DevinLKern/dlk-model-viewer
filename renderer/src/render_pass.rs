use crate::{Error, ImageHandle, Renderer, Result};

use ash::vk;

pub struct TargetImage {
    pub handle: ImageHandle,
    pub resolve_hanlde: Option<ImageHandle>,
}

pub struct RenderTarget {
    pub color_images: Box<[TargetImage]>,
    pub depth_image: Option<ImageHandle>,
    pub render_area: vk::Rect2D,
}

impl RenderTarget {
    #[inline]
    pub fn get_default_scissor_and_viewport(&self) -> (vk::Rect2D, vk::Viewport) {
        let scissor = self.render_area;
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: scissor.extent.width as f32,
            height: scissor.extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };

        return (scissor, viewport);
    }
}

#[allow(dead_code)]
pub struct Attachment {
    pub load_op: vk::AttachmentLoadOp,
    pub store_op: vk::AttachmentStoreOp,
    pub clear_val: vk::ClearValue,
    pub final_layout: vk::ImageLayout,
}

#[allow(dead_code)]
pub struct RenderPass {
    pub color: Box<[Attachment]>,
    pub depth: Option<Attachment>,
    // pub stencil: Option<Attachment>,
}

impl RenderPass {
    pub fn begin_rendering(
        &self,
        target: &RenderTarget,
        renderer: &mut Renderer,
        cmd: vk::CommandBuffer,
    ) -> Result<()> {
        debug_assert!(target.color_images.len() == self.color.len());
        debug_assert!(target.depth_image.is_some() && self.depth.is_some());

        let mut barriers = Vec::with_capacity(self.color.len() + 2);
        let mut color_attachments = Vec::with_capacity(self.color.len());
        let mut depth_attachment = vk::RenderingAttachmentInfo::default();

        for (color_attachment, color_target) in self.color.iter().zip(target.color_images.iter()) {
            let (resolve_mode, resolve_image_view, resolve_image_layout) =
                match color_target.resolve_hanlde {
                    Some(handle) => {
                        let img = renderer
                            .get_image_mut(handle)
                            .ok_or(Error::ResourceMissing)?;
                        let new_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
                        if img.layout != new_layout {
                            barriers.push(vk::ImageMemoryBarrier2 {
                                src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                                src_access_mask: vk::AccessFlags2::empty(),
                                dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                                old_layout: img.layout,
                                new_layout,
                                image: img.handle,
                                subresource_range: vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR,
                                    base_mip_level: 0,
                                    level_count: img.mip_level_count,
                                    base_array_layer: 0,
                                    layer_count: img.layer_count,
                                },
                                ..Default::default()
                            });
                            img.layout = new_layout;
                        }
                        (vk::ResolveModeFlags::AVERAGE, img.view, img.layout)
                    }
                    None => (
                        vk::ResolveModeFlags::default(),
                        vk::ImageView::default(),
                        vk::ImageLayout::default(),
                    ),
                };
            let img = renderer
                .get_image_mut(color_target.handle)
                .ok_or(Error::ResourceMissing)?;

            let new_layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
            if img.layout != new_layout {
                barriers.push(vk::ImageMemoryBarrier2 {
                    // NOTE: TOP_OF_PIPE should not be hard coded.
                    // In the future, multiple passes will be used and TOP_OF_PIPE will not always be correct
                    src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    dst_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    old_layout: img.layout,
                    new_layout,
                    image: img.handle,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        base_mip_level: 0,
                        level_count: img.mip_level_count,
                        base_array_layer: 0,
                        layer_count: img.layer_count,
                    },
                    ..Default::default()
                });
                img.layout = new_layout;
            }

            color_attachments.push(vk::RenderingAttachmentInfo {
                image_view: img.view,
                image_layout: img.layout,
                load_op: color_attachment.load_op,
                store_op: color_attachment.store_op,
                clear_value: color_attachment.clear_val,
                resolve_mode,
                resolve_image_view,
                resolve_image_layout,
                ..Default::default()
            });
        }
        if let (Some(depth_image), Some(attachment)) = (target.depth_image, &self.depth) {
            let img = renderer
                .get_image_mut(depth_image)
                .ok_or(Error::ResourceMissing)?;
            let new_layout = vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL;

            if img.layout != new_layout {
                let aspect_mask = if matches!(
                    img.format,
                    vk::Format::D32_SFLOAT_S8_UINT
                        | vk::Format::D24_UNORM_S8_UINT
                        | vk::Format::D16_UNORM_S8_UINT
                ) {
                    vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
                } else {
                    vk::ImageAspectFlags::DEPTH
                };
                // NOTE: This renderer currently only uses combined depth stencil images.
                barriers.push(vk::ImageMemoryBarrier2 {
                    src_stage_mask: vk::PipelineStageFlags2::TOP_OF_PIPE,
                    src_access_mask: vk::AccessFlags2::empty(),
                    dst_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    old_layout: img.layout,
                    new_layout,
                    image: img.handle,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask,
                        base_mip_level: 0,
                        level_count: img.mip_level_count,
                        base_array_layer: 0,
                        layer_count: img.layer_count,
                    },
                    ..Default::default()
                });
                img.layout = new_layout;
            }

            depth_attachment = vk::RenderingAttachmentInfo {
                image_view: img.view,
                image_layout: img.layout,
                load_op: attachment.load_op,
                store_op: attachment.store_op,
                clear_value: attachment.clear_val,
                ..Default::default()
            };
        }
        let dependency_info = vk::DependencyInfo {
            image_memory_barrier_count: barriers.len() as u32,
            p_image_memory_barriers: barriers.as_ptr(),
            ..Default::default()
        };
        unsafe { renderer.device.cmd_pipeline_barrier2(cmd, &dependency_info) };

        // begin dynamic rendering
        let rendering_info = vk::RenderingInfo {
            render_area: target.render_area,
            layer_count: 1,
            view_mask: 0,
            color_attachment_count: color_attachments.len() as u32,
            p_color_attachments: color_attachments.as_ptr(),
            p_depth_attachment: if self.depth.is_some() {
                &depth_attachment
            } else {
                std::ptr::null()
            },
            ..Default::default()
        };

        unsafe {
            renderer.device.cmd_begin_rendering(cmd, &rendering_info);
        };

        Ok(())
    }

    pub fn end_rendering(
        &self,
        target: &RenderTarget,
        renderer: &mut Renderer,
        cmd: vk::CommandBuffer,
    ) -> Result<()> {
        debug_assert!(target.color_images.len() == self.color.len());
        debug_assert!(target.depth_image.is_some() && self.depth.is_some());

        // end rendering
        unsafe {
            renderer.device.cmd_end_rendering(cmd);
        }

        let mut barriers = Vec::with_capacity(self.color.len() + 2);
        for (attachment, color_target) in self.color.iter().zip(target.color_images.iter()) {
            let img = match color_target.resolve_hanlde {
                Some(img) => img,
                None => color_target.handle,
            };
            let img = renderer.get_image_mut(img).ok_or(Error::ResourceMissing)?;

            if img.layout == attachment.final_layout {
                continue;
            }

            barriers.push(vk::ImageMemoryBarrier2 {
                src_stage_mask: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                src_access_mask: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                dst_stage_mask: vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
                dst_access_mask: vk::AccessFlags2::empty(),
                old_layout: img.layout,
                new_layout: attachment.final_layout,
                image: img.handle,
                subresource_range: vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: img.mip_level_count,
                    base_array_layer: 0,
                    layer_count: img.layer_count,
                },
                ..Default::default()
            });
            img.layout = attachment.final_layout;
        }

        if let (Some(attachment), Some(depth_target)) = (&self.depth, target.depth_image) {
            let img = renderer
                .get_image_mut(depth_target)
                .ok_or(Error::ResourceMissing)?;
            let aspect_mask = if matches!(
                img.format,
                vk::Format::D32_SFLOAT_S8_UINT
                    | vk::Format::D24_UNORM_S8_UINT
                    | vk::Format::D16_UNORM_S8_UINT
            ) {
                vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
            } else {
                vk::ImageAspectFlags::DEPTH
            };
            if img.layout != attachment.final_layout {
                // NOTE: This renderer currently only uses combined depth stencil images.
                barriers.push(vk::ImageMemoryBarrier2 {
                    src_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    src_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
                    dst_stage_mask: vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
                    dst_access_mask: vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ,
                    old_layout: img.layout,
                    new_layout: attachment.final_layout,
                    image: img.handle,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask,
                        base_mip_level: 0,
                        level_count: img.mip_level_count,
                        base_array_layer: 0,
                        layer_count: img.layer_count,
                    },
                    ..Default::default()
                });
                img.layout = attachment.final_layout;
            }
        }

        let dependency_info = vk::DependencyInfo {
            image_memory_barrier_count: barriers.len() as u32,
            p_image_memory_barriers: barriers.as_ptr(),
            ..Default::default()
        };

        unsafe { renderer.device.cmd_pipeline_barrier2(cmd, &dependency_info) };

        Ok(())
    }
}
