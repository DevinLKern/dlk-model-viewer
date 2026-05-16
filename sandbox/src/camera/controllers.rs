use math::{Quat, Vec2, Vec3, Zero};

use crate::{
    Camera,
    constants::{ENGINE_FORWARDS, ENGINE_RIGHT, ENGINE_UP},
};

pub trait CameraController {
    fn update(&mut self, camera: &mut Camera, sensitivity: f64, dt: f64);
}

pub struct FpsCameraController {
    pub yaw: f64,
    pub pitch: f64,
    pub rotation_delta: Vec2<f64>,
    pub movement: Vec3<f32>,
    pub zoom_delta: f32,
}

impl FpsCameraController {
    pub fn new() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            rotation_delta: Vec2::ZERO,
            movement: Vec3::ZERO,
            zoom_delta: 0.0,
        }
    }
    #[inline]
    pub fn rotate(&mut self, dx: f64, dy: f64) {
        *self.rotation_delta.x_mut() += dx;
        *self.rotation_delta.y_mut() += dy;
    }
    #[inline]
    pub fn r#move(&mut self, offset: Vec3<f32>) {
        self.movement.add_assign(offset);
    }
}

impl CameraController for FpsCameraController {
    fn update(&mut self, camera: &mut Camera, sensitivity: f64, dt: f64) {
        self.yaw += sensitivity * self.rotation_delta.x();
        self.pitch += sensitivity * self.rotation_delta.y();
        const LIMIT: f64 = std::f64::consts::FRAC_PI_2 - 0.001;
        self.pitch = self.pitch.clamp(-LIMIT, LIMIT);

        let q_yaw = Quat::unit_from_angle_axis(self.yaw as f32, ENGINE_UP);
        let right = q_yaw.rotate_vec(ENGINE_RIGHT);
        let q_pitch = Quat::unit_from_angle_axis(self.pitch as f32, right);
        camera.transform.orientation = q_pitch.mul(q_yaw);

        self.movement.scale_assign(dt as f32);
        let movement = self
            .movement
            .scaled_nonuniform(ENGINE_RIGHT.add(ENGINE_FORWARDS).abs());
        camera.transform.translate_local(movement);
        let movement = self.movement.scaled_nonuniform(ENGINE_UP);

        camera.transform.translate_global(movement);

        let zoom = camera.get_zoom();
        let new_zoom = (zoom + self.zoom_delta).clamp(1.0, 4.0);
        camera.set_zoom(new_zoom);

        self.rotation_delta = Vec2::ZERO;
        self.movement = Vec3::ZERO;
        self.zoom_delta = 0.0;
    }
}

#[allow(unused)]
pub struct OrbitCameraController {
    pub target: Vec3<f32>,
    pub delta_radius: f32,
    pub rotation_delta: Vec2<f64>,
    pub zoom_delta: f32,
}

#[allow(unused)]
impl OrbitCameraController {
    pub const fn new(target: Vec3<f32>) -> Self {
        Self {
            target,
            delta_radius: 0.0,
            rotation_delta: Vec2::ZERO,
            zoom_delta: 0.0,
        }
    }
    #[inline]
    pub const fn update_target(&mut self, new_target: Vec3<f32>) {
        self.target = new_target;
    }
    #[inline]
    pub const fn rotate(&mut self, dx: f64, dy: f64) {
        *self.rotation_delta.x_mut() += dx;
        *self.rotation_delta.y_mut() += dy;
    }
    #[inline]
    pub const fn r#move(&mut self, dr: f32) {
        self.delta_radius += dr;
    }
}

impl CameraController for OrbitCameraController {
    fn update(&mut self, camera: &mut Camera, sensitivity: f64, dt: f64) {
        let dx = sensitivity * self.rotation_delta.x();
        let dy = sensitivity * self.rotation_delta.y();
        self.rotation_delta = Vec2::ZERO;

        let up = camera.transform.orientation.rotate_vec(ENGINE_UP);
        let right = camera.transform.orientation.rotate_vec(ENGINE_RIGHT);
        let q_yaw = Quat::unit_from_angle_axis(dx as f32, up);
        let q_pitch = Quat::unit_from_angle_axis(dy as f32, right);
        let rotation = q_yaw.mul(q_pitch);

        camera.transform.rotate_global(rotation, self.target);
        camera.look_at(self.target, up);

        let radius = camera.transform.position.sub(self.target).length();
        let new_radius = (radius - (self.delta_radius * dt as f32)).max(0.1);
        let allowed_dr = radius - new_radius;
        camera
            .transform
            .translate_local(ENGINE_FORWARDS.scaled(allowed_dr));
        self.delta_radius = 0.0;

        let zoom = camera.get_zoom();
        let new_zoom = (zoom + self.zoom_delta).clamp(1.0, 4.0);
        camera.set_zoom(new_zoom);
        self.zoom_delta = 0.0;
    }
}
