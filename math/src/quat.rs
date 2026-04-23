use crate::mat3::Mat3;
use crate::mat4::Mat4;
use crate::traits::{Identity, Zero};
use crate::vec3::Vec3;

#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub struct Quat {
    w: f32,
    v: Vec3<f32>,
}

impl Identity for Quat {
    const IDENTITY: Self = Self {
        w: 1.0,
        v: Vec3::ZERO,
    };
}

#[allow(dead_code)]
impl Quat {
    pub fn from_basis(right: Vec3<f32>, up: Vec3<f32>, forward: Vec3<f32>) -> Self {
        let right = right.normalized();
        let up = up.normalized();
        let forward = forward.normalized();

        let m00 = right.x();
        let m01 = up.x();
        let m02 = forward.x();

        let m10 = right.y();
        let m11 = up.y();
        let m12 = forward.y();

        let m20 = right.z();
        let m21 = up.z();
        let m22 = forward.z();

        let trace = m00 + m11 + m22;

        if trace > 0.0 {
            let s = (trace + 1.0).sqrt() * 2.0;
            let w = 0.25 * s;
            let x = (m21 - m12) / s;
            let y = (m02 - m20) / s;
            let z = (m10 - m01) / s;
            Self::from_xyzw(x, y, z, w)
        } else if m00 > m11 && m00 > m22 {
            let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
            let w = (m21 - m12) / s;
            let x = 0.25 * s;
            let y = (m01 + m10) / s;
            let z = (m02 + m20) / s;
            Self::from_xyzw(x, y, z, w)
        } else if m11 > m22 {
            let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
            let w = (m02 - m20) / s;
            let x = (m01 + m10) / s;
            let y = 0.25 * s;
            let z = (m12 + m21) / s;
            Self::from_xyzw(x, y, z, w)
        } else {
            let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
            let w = (m10 - m01) / s;
            let x = (m02 + m20) / s;
            let y = (m12 + m21) / s;
            let z = 0.25 * s;
            Self::from_xyzw(x, y, z, w)
        }
    }
    #[inline]
    pub fn unit_from_angle_axis(angle_rad: f32, axis: Vec3<f32>) -> Self {
        let half = angle_rad * 0.5;
        let (s, c) = half.sin_cos(); // NOTE: sin_cos is not cost

        Self {
            w: c,
            v: axis.normalized().scaled(s), // normalized is not const
        }
    }
    #[inline]
    pub const fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            w,
            v: Vec3::new(x, y, z),
        }
    }
    #[inline]
    pub fn unit_from_wxyz(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self::from_xyzw(x, y, z, w).normalized()
    }
    #[inline]
    pub const fn w(&self) -> f32 {
        self.w
    }
    #[inline]
    pub const fn x(&self) -> f32 {
        self.v.x()
    }
    #[inline]
    pub const fn y(&self) -> f32 {
        self.v.y()
    }
    #[inline]
    pub const fn z(&self) -> f32 {
        self.v.z()
    }
    #[inline]
    const fn w_mut(&mut self) -> &mut f32 {
        &mut self.w
    }
    #[inline]
    const fn x_mut(&mut self) -> &mut f32 {
        self.v.x_mut()
    }
    #[inline]
    const fn y_mut(&mut self) -> &mut f32 {
        self.v.y_mut()
    }
    #[inline]
    const fn z_mut(&mut self) -> &mut f32 {
        self.v.z_mut()
    }
    #[inline]
    pub fn angle_radians(&self) -> f32 {
        2.0 * self.w.acos()
    }
    pub fn axis(&self) -> Vec3<f32> {
        let a = self.w.acos().sin();
        self.v.scaled(1.0 / a)
    }
    #[inline]
    pub const fn into_mat3(self) -> Mat3<f32> {
        let w = self.w();
        let x = self.x();
        let y = self.y();
        let z = self.z();
        Mat3::from_rows(
            Vec3::new(
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - w * z),
                2.0 * (x * z + w * y),
            ),
            Vec3::new(
                2.0 * (x * y + w * z),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - w * x),
            ),
            Vec3::new(
                2.0 * (x * z - w * y),
                2.0 * (y * z + w * x),
                1.0 - 2.0 * (x * x + y * y),
            ),
        )
    }
    #[inline]
    pub const fn as_mat3(&self) -> Mat3<f32> {
        self.into_mat3()
    }
    #[inline]
    pub const fn into_mat4(self) -> Mat4<f32> {
        self.into_mat3().into_mat4(1.0)
    }
    #[inline]
    pub const fn as_mat4(&self) -> Mat4<f32> {
        self.into_mat4()
    }
    #[inline]
    pub const fn conjugate(&self) -> Self {
        Self {
            w: self.w,
            v: Vec3::ZERO.sub(self.v),
        }
    }
    #[inline]
    pub const fn length_squared(&self) -> f32 {
        self.w * self.w + self.v.length_squared()
    }
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt() // NOTE: sqrt is not const
    }
    pub fn normalized(&self) -> Self {
        let len = self.length();

        if len == 0.0 {
            return Self {
                w: 1.0,
                v: Vec3::ZERO,
            };
        }

        let inv = 1.0 / len;

        Self {
            w: self.w() * inv,
            v: self.v.scaled(inv),
        }
    }
    // this is shorthand for p * v * p^-1
    pub const fn rotate_vec(&self, v: Vec3<f32>) -> Vec3<f32> {
        let inv = self.inverse();
        let v = Self { w: 0.0, v: v };

        self.mul(v).mul(inv).v
    }
    #[inline]
    pub const fn scaled(&self, s: f32) -> Self {
        Self {
            w: self.w() * s,
            v: self.v.scaled(s),
        }
    }
    #[inline]
    pub const fn inverse(&self) -> Self {
        let c = self.conjugate();
        let s = 1.0 / self.length_squared();

        c.scaled(s)
    }
    #[inline]
    pub const fn added(&self, rhs: &Self) -> Self {
        Self {
            w: self.w() + rhs.w,
            v: self.v.add(rhs.v),
        }
    }
    #[inline]
    pub const fn mul(&self, rhs: Self) -> Self {
        let w = self.w() * rhs.w() - self.v.dot(rhs.v);
        let v = self
            .v
            .scaled(rhs.w())
            .add(rhs.v.scaled(self.w).add(self.v.cross(rhs.v)));

        Self { w, v }
    }
    #[inline]
    pub const fn mul_assign(&mut self, rhs: Self) {
        *self = self.mul(rhs);
    }
}

impl std::fmt::Display for Quat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{angle: {}, v: {}}}", self.angle_radians(), self.axis())
    }
}

impl PartialEq for Quat {
    fn eq(&self, other: &Self) -> bool {
        self.w() == other.w() && self.v == other.v
    }
    fn ne(&self, other: &Self) -> bool {
        self.w() != other.w() || self.v != other.v
    }
}

#[cfg(test)]
mod tests {
    use crate::{quat::Quat, vec3::Vec3};

    #[test]
    fn angle_axis_tests() {
        let _angle: f32 = 0.5;
        let _axis = Vec3::<f32>::new(1.0, 2.0, 3.0);
    }

    #[test]
    fn inversion() {
        let q = Quat::unit_from_angle_axis(0.5, Vec3::new(1.0, 0.0, 0.0));
        let result = q.inverse();
        let expected = Quat::unit_from_angle_axis(-0.5, Vec3::new(1.0, 0.0, 0.0));

        assert_eq!(result, expected);
    }

    #[test]
    fn multiplication1() {
        let a = Quat::from_xyzw(1.0, -2.0, 3.0, -4.0);
        let b = Quat::from_xyzw(5.0, 6.0, -7.0, 8.0);

        let result = a.mul(b);
        let expected = Quat::from_xyzw(-16.0, -18.0, 68.0, -4.0);
        assert_eq!(result, expected);
    }

    #[test]
    fn rotation() {
        let p = Vec3::new(0.0, 1.0, 0.0);
        let q = Quat::unit_from_angle_axis(90f32.to_radians(), Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(q.rotate_vec(p), Vec3::new(0.0, 0.0, 1.0));

        let p = Vec3::new(0.0, 0.0, 1.0);
        let q = Quat::unit_from_angle_axis(-90f32.to_radians(), Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(q.rotate_vec(p), Vec3::new(-1.0, 0.0, 0.0));

        let p = Vec3::new(1.0, 0.0, 0.0);
        let q = Quat::unit_from_angle_axis(-90f32.to_radians(), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(q.rotate_vec(p), Vec3::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn conversion_to_matrix() {
        // let q = Quaternion::unit_from_angle_axis(0.5, Vec3::new(1.0, 0.0, 0.0));

        // assert_eq!(q.into_mat4(), m);
    }
}
