use math::{Mat4, Vec4};

#[derive(Debug)]
pub struct PerspectiveProjection {
    pub zoom: f32,
    fov_y: f32,
    pub aspect_ratio: f32,
    near: f32,
    far: f32,
}

impl PerspectiveProjection {
    #[inline]
    pub const fn new(fov_y: f32) -> Self {
        Self {
            zoom: 1.0,
            fov_y,
            aspect_ratio: 1.0,
            near: 0.1,
            far: 10.0,
        }
    }
    #[inline]
    pub fn projection_matrix(&self) -> Mat4<f32> {
        let n = self.near;
        let f = self.far;
        let fov_y = self.fov_y / self.zoom;
        let half_tan = (fov_y.to_radians() / 2.0).tan();

        Mat4::from_cols(
            Vec4::new(1.0 / (self.aspect_ratio * half_tan), 0.0, 0.0, 0.0),
            // vulkan y flip
            Vec4::new(0.0, -1.0 / half_tan, 0.0, 0.0),
            Vec4::new(0.0, 0.0, f / (n - f), -1.0),
            Vec4::new(0.0, 0.0, (f * n) / (n - f), 0.0),
        )
    }
}

#[derive(Debug)]
pub struct OrthographicProjection {
    pub zoom: f32,
    pub aspect_ratio: f32,
    l: f32,
    r: f32,
    t: f32,
    b: f32,
    n: f32,
    f: f32,
}

impl OrthographicProjection {
    pub const fn new(width: f32, height: f32, depth: f32) -> Self {
        let width = width / 2.0;
        let height = height / 2.0;

        Self {
            zoom: 1.0,
            aspect_ratio: 1.0,
            r: width,
            l: -width,
            t: height,
            b: -height,
            n: 0.01,
            f: depth,
        }
    }

    pub fn projection_matrix(&self) -> Mat4<f32> {
        let half_height = (self.b.abs() + self.t.abs()) * 0.5;
        let half_width = half_height * self.aspect_ratio;

        let l = -half_width;
        let r = half_width;

        let sx = 2.0 / (r - l);
        // vulkan y flip
        let sy = -2.0 / (self.t - self.b);
        let sz = -1.0 / (self.f - self.n);

        let tx = -(self.r + self.l) / (self.r - self.l);
        let ty = -(self.t + self.b) / (self.t - self.b);
        let tz = -self.n / (self.f - self.n);

        let zf = self.zoom;

        Mat4::from_cols(
            Vec4::new(sx * zf, 0.0, 0.0, 0.0),
            Vec4::new(0.0, sy * zf, 0.0, 0.0),
            Vec4::new(0.0, 0.0, sz, 0.0),
            Vec4::new(tx, ty, tz, 1.0),
        )
    }
}
