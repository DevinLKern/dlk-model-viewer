use math::{Mat4, Vec4};


#[derive(Debug)]
pub struct PerspectiveProjection {
    // zoom: f32,
    fov_y: f32,
    aspect_ratio: f32,
    near: f32,
    far: f32,
}

impl PerspectiveProjection {
    #[inline]
    pub const fn new(fov_y: f32, aspect_ratio: f32) -> Self {
        Self {
            // zoom: 1.0,
            fov_y,
            aspect_ratio,
            near: 0.1,
            far: 1000.0,
        }
    }
    #[inline]
    pub const fn update_aspect_ratio(&mut self, new_aspect_ratio: f32) {
        self.aspect_ratio = new_aspect_ratio;
    }
    #[inline]
    pub fn projection_matrix(&self) -> Mat4<f32> {
        let n = self.near;
        let f = self.far;
        const R: f32 = vulkan::VK_VIEW_VOLUME_RIGHT;
        const L: f32 = vulkan::VK_VIEW_VOLUME_LEFT;
        const T: f32 = vulkan::VK_VIEW_VOLUME_TOP;
        const B: f32 = vulkan::VK_VIEW_VOLUME_BOTTOM;

        let half_tan = (self.fov_y.to_radians() / 2.0).tan();

        Mat4::from_cols(
            Vec4::new(1.0 / (self.aspect_ratio * half_tan), 0.0, 0.0, 0.0),
            Vec4::new(0.0, -1.0 / half_tan, 0.0, 0.0),
            Vec4::new((R + L) / (R - L), (T + B) / (T - B), -f / (f - n), -1.0),
            Vec4::new(0.0, 0.0, -f * n / (f - n), 0.0),
        )
    }
}

#[derive(Debug)]
pub struct OrthographicProjection {
    aspect_ratio: f32,
    l: f32,
    r: f32,
    t: f32,
    b: f32,
    n: f32,
    f: f32,
}

impl OrthographicProjection { 
    pub fn new(width: f32, height: f32, depth: f32) -> Self {
        let width = width / 2.0;
        let height = height / 2.0;
        
        Self {
            aspect_ratio: 1.0,
            l: -width,
            r: width,
            t: -height,
            b: height,
            n: 0.0,
            f: -depth,
        }
    }

    pub fn projection_matrix(&self) -> Mat4<f32> {
        let half_height = (self.b.abs() + self.t.abs()) * 0.5;
        let half_width = half_height * self.aspect_ratio;

        let l = -half_width;
        let r = half_width;
        
        let sx = -2.0 / (r - l);
        let sy = -2.0 / (self.b - self.t);
        let sz = 1.0 / (self.f - self.n);
        
        let tx = -(self.r + self.l) / (self.r - self.l);
        let ty = -(self.b + self.t) / (self.b - self.t);
        let tz = self.n / (self.f - self.n);

        Mat4::from_cols(
            Vec4::new(sx, 0.0, 0.0, 0.0),
            Vec4::new(0.0, sy, 0.0, 0.0),
            Vec4::new(0.0, 0.0, sz, 0.0),
            Vec4::new(tx, ty, tz, 1.0),
        )
    }

    pub fn update_aspect_ratio(&mut self, new_aspect_ratio: f32) {
        self.aspect_ratio = new_aspect_ratio;
    }
}
