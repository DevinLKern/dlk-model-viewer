use crate::{Identity, Mat4, Quat, Vec3, Zero};

#[derive(Debug)]
pub struct RigidTransform {
    pub position: Vec3<f32>,
    pub orientation: Quat,
}

impl RigidTransform {
    #[inline]
    pub const fn new(position: Vec3<f32>, orientation: Quat) -> Self {
        Self {
            position,
            orientation,
        }
    }
    #[inline]
    pub const fn inv(&self) -> Self {
        let inv_rot = self.orientation.inverse();
        let inv_pos = inv_rot.rotate_vec(self.position.scaled(-1.0));
        Self::new(inv_pos, inv_rot)
    }
    #[inline]
    pub const fn translate_global(&mut self, offset: Vec3<f32>) {
        self.position.add_assign(offset);
    }
    #[inline]
    pub const fn translate_local(&mut self, offset: Vec3<f32>) {
        let offset = self.orientation.rotate_vec(offset);
        self.position.add_assign(offset);
    }
    pub const fn rotate_global(&mut self, rotation: Quat, pivot: Vec3<f32>) {
        self.orientation = rotation.mul(self.orientation);
        self.position.sub_assign(pivot);
        self.position = rotation.rotate_vec(self.position);
        self.position.add_assign(pivot);
    }
    #[inline]
    pub const fn rotate_local(&mut self, rotation: Quat) {
        self.orientation = self.orientation.mul(rotation);
    }
    #[inline]
    pub const fn get_translation_matrix(&self) -> Mat4<f32> {
        Mat4::translation(self.position)
    }
    #[inline]
    pub const fn get_rotation_matrix(&self) -> Mat4<f32> {
        self.orientation.as_mat4()
    }
    #[inline]
    pub const fn as_mat4(&self) -> Mat4<f32> {
        let t = self.get_translation_matrix();
        let r = self.get_rotation_matrix();

        t.mul(&r)
    }
    #[inline]
    pub const fn into_mat4(self) -> Mat4<f32> {
        self.as_mat4()
    }
}

impl PartialEq for RigidTransform {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position && self.orientation == other.orientation
    }
    fn ne(&self, other: &Self) -> bool {
        self.position != other.position || self.orientation != other.orientation
    }
}

impl Default for RigidTransform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
        }
    }
}

#[allow(dead_code)]
trait HasRigidTransform {
    fn position(&self) -> &Vec3<f32>;
    fn orientation(&self) -> &Quat;
    fn position_and_orientation(&self) -> &RigidTransform;
}
