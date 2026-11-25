use crate::{Image, ImageViewDesc, Renderer, Result};

use ash::vk;

pub trait RenderStorage {
    type ImageHandle: Copy + Clone;
    fn get_image(&self, image_handle: Self::ImageHandle) -> Option<&Image>;
    fn get_image_mut(&mut self, image_handle: Self::ImageHandle) -> Option<&mut Image>;
}

pub struct TargetImage {
    pub image: vk::Image,
    pub image_layout: vk::ImageLayout,
    pub image_format: vk::Format,
    pub image_view: vk::ImageView,
    pub image_view_desc: ImageViewDesc,
}

impl TargetImage {
    pub(crate) fn new(image: &mut Image, image_view_desc: ImageViewDesc) -> Result<Self> {
        let image_view = image
            .create_image_view(image_view_desc)?;
        Ok(Self {
            image: image.handle(),
            image_layout: image.layout(),
            image_format: image.format(),
            image_view,
            image_view_desc,
        })
    }
}

pub struct ResolvableTargetImage {
    pub main: TargetImage,
    pub resolve: Option<TargetImage>,
}

// RenderTargets are genenrally built by a builder class.
// TargetImage descriptions are added to the constructor and then the
// Struct::build(&self, storage: &Storage) -> Option<RenderTarget>
// method should resolve all the descriptions or return None.

pub struct RenderTarget {
    pub color_images: Box<[ResolvableTargetImage]>,
    pub depth_image: Option<ResolvableTargetImage>,
    pub render_area: vk::Rect2D,
}

impl RenderTarget {
    #[inline]
    pub fn color_images(&self) -> &[ResolvableTargetImage] {
        &self.color_images
    }
    #[inline]
    pub fn depth_image(&self) -> Option<&ResolvableTargetImage> {
        self.depth_image.as_ref()
    }
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
    ) {
        #[inline]
        fn populate_rendering_attachment_info<'a>(
            target_image: &ResolvableTargetImage,
            attachment: &Attachment,
        ) -> vk::RenderingAttachmentInfo<'a> {
            let (resolve_mode, resolve_image_view, resolve_image_layout) =
                if let Some(resolve_image) = &target_image.resolve {
                    (
                        vk::ResolveModeFlags::AVERAGE,
                        resolve_image.image_view,
                        resolve_image.image_layout,
                    )
                } else {
                    (
                        vk::ResolveModeFlags::empty(),
                        vk::ImageView::null(),
                        vk::ImageLayout::UNDEFINED,
                    )
                };

            vk::RenderingAttachmentInfo {
                image_view: target_image.main.image_view,
                image_layout: target_image.main.image_layout,
                load_op: attachment.load_op,
                store_op: attachment.store_op,
                clear_value: attachment.clear_val,
                resolve_mode,
                resolve_image_view,
                resolve_image_layout,
                ..Default::default()
            }
        }

        debug_assert!(target.color_images().len() == self.color.len());
        debug_assert!(target.depth_image().is_some() && self.depth.is_some());

        let mut color_attachments = Vec::with_capacity(self.color.len());
        for (color_target, color_attachment) in target.color_images().iter().zip(self.color.iter())
        {
            let attachment_info =
                populate_rendering_attachment_info(color_target, color_attachment);
            color_attachments.push(attachment_info);
        }
        let depth_attachment_info = if let Some(depth_attachment) = &self.depth {
            // unwrap_unchecked should be because of assert at method start
            let depth_target = unsafe { target.depth_image().unwrap_unchecked() };
            populate_rendering_attachment_info(depth_target, depth_attachment)
        } else {
            vk::RenderingAttachmentInfo::default()
        };

        // begin dynamic rendering
        let rendering_info = vk::RenderingInfo {
            render_area: target.render_area,
            layer_count: 1,
            view_mask: 0,
            color_attachment_count: color_attachments.len() as u32,
            p_color_attachments: color_attachments.as_ptr(),
            p_depth_attachment: if self.depth.is_some() {
                &depth_attachment_info
            } else {
                std::ptr::null()
            },
            ..Default::default()
        };

        unsafe {
            renderer.device.cmd_begin_rendering(cmd, &rendering_info);
        };
    }
    #[inline]
    pub fn end_rendering(&self, renderer: &Renderer, cmd: vk::CommandBuffer) {
        // end rendering
        unsafe {
            renderer.device.cmd_end_rendering(cmd);
        }
    }
}
