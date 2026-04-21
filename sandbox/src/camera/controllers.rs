use math::{Vec2, Vec3, Quat, Zero};

use crate::{Camera, constants::{WORLD_FORWARDS, WORLD_RIGHT, WORLD_UP}};

pub trait CameraController {
    fn update(&mut self, camera: &mut Camera, sensitivity: f32, dt: f32);
}

pub struct FpsCameraController {
    pub yaw: f32,
    pub pitch: f32,
    pub rotation_delta: Vec2<f32>,
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
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        *self.rotation_delta.x_mut() += dx;
        *self.rotation_delta.y_mut() += dy;
    }
    #[inline]
    pub fn move_local(&mut self, offset: Vec3<f32>) {
        self.movement.add_assign(offset);
    }
}

impl CameraController for FpsCameraController {
    fn update(&mut self, camera: &mut Camera, sensitivity: f32, dt: f32) {
        self.yaw += -sensitivity * self.rotation_delta.x() * dt;
        self.pitch += -sensitivity * self.rotation_delta.y() * dt;

        let q_yaw = Quat::unit_from_angle_axis(self.yaw, WORLD_UP);

        const LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.001;
        self.pitch = self.pitch.clamp(-LIMIT, LIMIT);
        
        let right = q_yaw.rotate_vec(WORLD_RIGHT);
        let q_pitch = Quat::unit_from_angle_axis(self.pitch, right); 
        camera.transform.orientation =  q_pitch.mul(q_yaw);
        camera.transform.translate_local(self.movement.scaled(dt));

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
    pub rotation_delta: Vec2<f32>,
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
    pub const fn rotate(&mut self, dx: f32, dy: f32) {
        *self.rotation_delta.x_mut() += dx;
        *self.rotation_delta.y_mut() += dy;
    }
    #[inline]
    pub const fn r#move(&mut self, dr: f32) {
        self.delta_radius += dr;
    }
}

impl CameraController for OrbitCameraController {
    fn update(&mut self, camera: &mut Camera, sensitivity: f32, dt: f32) {
        let dx = self.rotation_delta.x() * sensitivity * dt;
        let dy = self.rotation_delta.y() * sensitivity * dt;
        self.rotation_delta = Vec2::ZERO;
    
        let up = camera.transform.orientation.rotate_vec(WORLD_UP);
        let right = camera.transform.orientation.rotate_vec(WORLD_RIGHT);
        let q_yaw = Quat::unit_from_angle_axis(dx, up);
        let q_pitch = Quat::unit_from_angle_axis(dy, right);
        let rotation = q_yaw.mul(q_pitch);
        
        camera.transform.rotate_global(rotation, self.target);
        camera.look_at(self.target, up);

        let radius = camera.transform.position.sub(self.target).length();
        let new_radius = (radius - (self.delta_radius * dt)).max(0.1);
        let allowed_dr = radius - new_radius;
        camera.transform.translate_local(WORLD_FORWARDS.scaled(allowed_dr));
        self.delta_radius = 0.0;

        let zoom = camera.get_zoom();
        let new_zoom = (zoom + self.zoom_delta).clamp(1.0, 4.0);
        camera.set_zoom(new_zoom);
        self.zoom_delta = 0.0;
    }
}
