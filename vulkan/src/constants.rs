use math::Vec3;

// TODO: this is wrong. left, right, top, bottom, near, and far are opinionated?
// Vulkan doesnt define right, left, ... but it does set view a view volume
#[allow(dead_code)]
pub const VK_VIEW_VOLUME_FAR: f32 = -1.0;
#[allow(dead_code)]
pub const VK_VIEW_VOLUME_NEAR: f32 = 0.0;
#[allow(dead_code)]
pub const VK_VIEW_VOLUME_RIGHT: f32 = 1.0;
#[allow(dead_code)]
pub const VK_VIEW_VOLUME_LEFT: f32 = -1.0;
#[allow(dead_code)]
pub const VK_VIEW_VOLUME_TOP: f32 = -1.0;
#[allow(dead_code)]
pub const VK_VIEW_VOLUME_BOTTOM: f32 = 1.0;

#[allow(dead_code)]
pub const VIEW_VOLUME_MIN: Vec3<f32> = Vec3::new(-1.0, -1.0, -1.0);
#[allow(dead_code)]
pub const VIEW_VOLUME_MAX: Vec3<f32> = Vec3::new(1.0, 1.0, 0.0);
