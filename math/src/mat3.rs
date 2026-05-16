use crate::Mat4;
use crate::Vec3;
use crate::Vec4;
use crate::traits::{Identity, One, Zero};

#[allow(dead_code)]
#[repr(transparent)]
#[derive(Copy, Clone, Debug)]
pub struct Mat3<T>([Vec3<T>; 3]);

impl<T> Mat3<T>
where
    T: Copy,
{
    #[inline]
    pub const fn from_rows(r0: Vec3<T>, r1: Vec3<T>, r2: Vec3<T>) -> Self {
        Self([
            Vec3::new(r0.x(), r1.x(), r2.x()),
            Vec3::new(r0.y(), r1.y(), r2.y()),
            Vec3::new(r0.z(), r1.z(), r2.z()),
        ])
    }
}

impl<T> Mat3<T> {
    #[inline]
    pub const fn from_cols(c0: Vec3<T>, c1: Vec3<T>, c2: Vec3<T>) -> Self {
        Self([c0, c1, c2])
    }
}

#[allow(dead_code)]
impl<T: Zero + Copy> Mat3<T> {
    fn scaling(s: Vec3<T>) -> Self {
        Self::from_cols(
            Vec3::new(s.x(), T::ZERO, T::ZERO),
            Vec3::new(T::ZERO, s.y(), T::ZERO),
            Vec3::new(T::ZERO, T::ZERO, s.z()),
        )
    }
}

impl<T: Zero> Zero for Mat3<T> {
    const ZERO: Self = Self::from_cols(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO);
}

impl<T: Zero + One> Identity for Mat3<T> {
    const IDENTITY: Self = Self::from_cols(
        Vec3::new(T::ONE, T::ZERO, T::ZERO),
        Vec3::new(T::ZERO, T::ONE, T::ZERO),
        Vec3::new(T::ZERO, T::ZERO, T::ONE),
    );
}

impl<T> Mat3<T>
where
    T: Copy,
{
    #[inline]
    pub const fn c0(&self) -> Vec3<T> {
        self.0[0]
    }
    #[inline]
    pub const fn c1(&self) -> Vec3<T> {
        self.0[1]
    }
    #[inline]
    pub const fn c2(&self) -> Vec3<T> {
        self.0[2]
    }
    #[inline]
    pub const fn r0(&self) -> Vec3<T> {
        Vec3::new(self.c0().x(), self.c1().x(), self.c2().x())
    }
    #[inline]
    pub const fn r1(&self) -> Vec3<T> {
        Vec3::new(self.c0().y(), self.c1().y(), self.c2().y())
    }
    #[inline]
    pub const fn r2(&self) -> Vec3<T> {
        Vec3::new(self.c0().z(), self.c1().z(), self.c2().z())
    }
    #[inline]
    pub const fn as_2d_arr(&self) -> [[T; 3]; 3] {
        [
            self.c0().into_arr(),
            self.c1().into_arr(),
            self.c2().into_arr(),
        ]
    }
}

impl<T> Mat3<T> {
    #[inline]
    pub const fn c0_mut(&mut self) -> &mut Vec3<T> {
        &mut self.0[0]
    }
    #[inline]
    pub const fn c1_mut(&mut self) -> &mut Vec3<T> {
        &mut self.0[1]
    }
    #[inline]
    pub const fn c2_mut(&mut self) -> &mut Vec3<T> {
        &mut self.0[2]
    }
}

impl Mat3<f32> {
    #[inline]
    pub const fn mul(&self, rhs: &Self) -> Mat3<f32> {
        let (r0, r1, r2) = (self.r0(), self.r1(), self.r2());

        Self::from_cols(
            Vec3::new(r0.dot(rhs.c0()), r1.dot(rhs.c0()), r2.dot(rhs.c0())),
            Vec3::new(r0.dot(rhs.c1()), r1.dot(rhs.c1()), r2.dot(rhs.c1())),
            Vec3::new(r0.dot(rhs.c2()), r1.dot(rhs.c2()), r2.dot(rhs.c2())),
        )
    }
    #[inline]
    pub const fn mul_vec(&self, v: Vec3<f32>) -> Vec3<f32> {
        self.c0()
            .scaled(v.x())
            .add(self.c1().scaled(v.y()))
            .add(self.c2().scaled(v.z()))
    }
    #[inline]
    pub const fn transposed(&self) -> Self {
        Self::from_rows(self.c0(), self.c1(), self.c2())
    }
    #[inline]
    pub const fn adjoint(&self) -> Self {
        let a = self.c0().x();
        let b = self.c1().x();
        let c = self.c2().x();

        let d = self.c0().y();
        let e = self.c1().y();
        let f = self.c2().y();

        let g = self.c0().z();
        let h = self.c1().z();
        let i = self.c2().z();

        let c0 = Vec3::new(e * i - f * h, f * g - d * i, d * h - e * g);

        let c1 = Vec3::new(c * h - b * i, a * i - c * g, b * g - a * h);

        let c2 = Vec3::new(b * f - c * e, c * d - a * f, a * e - b * d);

        Self::from_cols(c0, c1, c2)
    }
    pub const fn determinant(&self) -> f32 {
        let a = self.c0().x();
        let b = self.c0().y();
        let c = self.c0().z();

        let d = self.c1().x();
        let e = self.c1().y();
        let f = self.c1().z();

        let g = self.c2().x();
        let h = self.c2().y();
        let i = self.c2().z();

        (a * (e * i - f * h)) - (b * (d * i - g * f)) + (c * (d * h - e * g))
    }
    #[inline]
    pub const fn inverse(&self) -> Option<Self> {
        let mut adj = self.adjoint();
        let det = self.determinant();
        if det == 0.0 {
            return None;
        }
        let s = 1.0 / det;

        adj.c0_mut().scale_assign(s);
        adj.c1_mut().scale_assign(s);
        adj.c2_mut().scale_assign(s);

        Some(adj)
    }
}

impl<T: std::fmt::Display + Copy> std::fmt::Display for Mat3<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        write!(f, "{}, ", self.c0())?;
        write!(f, "{}, ", self.c1())?;
        write!(f, "{}", self.c2())?;
        write!(f, "]")
    }
}

impl<T: PartialEq + Copy> PartialEq for Mat3<T> {
    fn eq(&self, other: &Self) -> bool {
        self.c0() == other.c0() && self.c1() == other.c1() && self.c2() == other.c2()
    }
    fn ne(&self, other: &Self) -> bool {
        self.c0() != other.c0() || self.c1() != other.c1() || self.c2() != other.c2()
    }
}

impl<T> Mat3<T>
where
    T: Zero + One + Copy,
{
    pub const fn into_mat4(self, v: T) -> crate::Mat4<T> {
        crate::Mat4::from_cols(
            self.c0().into_vec4(),
            self.c1().into_vec4(),
            self.c2().into_vec4(),
            Vec4::new(T::ZERO, T::ZERO, T::ZERO, v),
        )
    }
    pub const fn as_mat4(&self, v: T) -> crate::Mat4<T> {
        Mat4::from_cols(
            self.c0().as_vec4(),
            self.c1().as_vec4(),
            self.c2().as_vec4(),
            Vec4::new(T::ZERO, T::ZERO, T::ZERO, v),
        )
    }
}

#[cfg(test)]
mod test {
    use super::Mat3;
    use super::Vec3;

    #[test]
    fn multiplication_scaling() {
        let s = Mat3::scaling(Vec3::new(2.0, 3.0, 4.0));

        let v = Mat3::from_cols(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );

        let result = v.mul(&s);

        let expected = Mat3::from_cols(
            Vec3::new(2.0, 4.0, 6.0),
            Vec3::new(12.0, 15.0, 18.0),
            Vec3::new(28.0, 32.0, 36.0),
        );

        assert_eq!(result, expected);
    }

    #[test]
    fn multiplication_chained() {
        let a = Mat3::from_rows(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );
        let b = Mat3::from_rows(
            Vec3::new(10.0, 11.0, 12.0),
            Vec3::new(13.0, 14.0, 15.0),
            Vec3::new(16.0, 17.0, 18.0),
        );
        let c = Mat3::from_rows(
            Vec3::new(19.0, 20.0, 21.0),
            Vec3::new(22.0, 23.0, 24.0),
            Vec3::new(25.0, 26.0, 27.0),
        );

        let result = a.mul(&b).mul(&c);

        // this is equivalent to a * b * c
        let expected = Mat3::from_rows(
            Vec3::new(5976.0, 6246.0, 6516.0),
            Vec3::new(14346.0, 14994.0, 15642.0),
            Vec3::new(22716.0, 23742.0, 24768.0),
        );

        assert_eq!(result, expected);
    }
    #[test]
    fn determinant() {
        let a = Mat3::from_rows(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );

        assert_eq!(a.determinant(), 0.0);

        let b = Mat3::from_rows(
            Vec3::new(1.0, 3.0, 1.0),
            Vec3::new(0.0, 3.0, 1.0),
            Vec3::new(4.0, 2.0, 0.0),
        );

        assert_eq!(b.determinant(), -2.0);
    }
    #[test]
    fn adjoint() {
        let a = Mat3::from_rows(
            Vec3::new(1.0, 3.0, 1.0),
            Vec3::new(0.0, 3.0, 1.0),
            Vec3::new(4.0, 2.0, 0.0),
        );

        let r1 = Mat3::from_rows(
            Vec3::new(-2.0, 2.0, 0.0),
            Vec3::new(4.0, -4.0, -1.0),
            Vec3::new(-12.0, 10.0, 3.0),
        );

        assert_eq!(a.adjoint(), r1);
    }
    #[test]
    fn inverse() {
        let a = Mat3::from_rows(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );
        let b = Mat3::from_rows(
            Vec3::new(1.0, 3.0, 1.0),
            Vec3::new(0.0, 3.0, 1.0),
            Vec3::new(4.0, 2.0, 0.0),
        );

        assert_eq!(a.inverse(), None);

        let r2 = Mat3::from_rows(
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(-2.0, 2.0, 0.5),
            Vec3::new(6.0, -5.0, -1.5),
        );
        assert_eq!(b.inverse(), Some(r2));
    }
    #[test]
    fn mul_vec() {
        let a = Mat3::from_rows(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::new(7.0, 8.0, 9.0),
        );
        let b = Mat3::from_rows(
            Vec3::new(10.0, 11.0, 12.0),
            Vec3::new(13.0, 14.0, 15.0),
            Vec3::new(16.0, 17.0, 18.0),
        );
        let c = Mat3::from_rows(
            Vec3::new(19.0, 20.0, 21.0),
            Vec3::new(22.0, 23.0, 24.0),
            Vec3::new(25.0, 26.0, 27.0),
        );

        let v = Vec3::new(28.0, 29.0, 30.0);
        let v1 = c.mul_vec(v);
        let v1 = b.mul_vec(v1);
        let v1 = a.mul_vec(v1);

        let v2 = a.mul(&b).mul(&c).mul_vec(v);

        assert_eq!(v1, v2);
    }
}
