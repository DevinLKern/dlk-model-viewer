use math::{Mat3, Vec3};

pub(crate) const ENGINE_RIGHT: Vec3<f32> = Vec3::new(1.0, 0.0, 0.0);
pub(crate) const ENGINE_UP: Vec3<f32> = Vec3::new(0.0, 1.0, 0.0);
pub(crate) const ENGINE_FORWARDS: Vec3<f32> = Vec3::new(0.0, 0.0, -1.0);
pub(crate) const TO_ENGINE: Mat3<f32> = Mat3::from_cols(ENGINE_RIGHT, ENGINE_UP, ENGINE_FORWARDS);
// pub(crate) const FROM_ENGINE: Mat3<f32> = TO_ENGINE.transposed();
