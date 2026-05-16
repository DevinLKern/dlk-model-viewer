use crate::Mat3;
use crate::traits::{Identity, One, Zero};
use crate::vec3::Vec3;
use crate::vec4::Vec4;

#[allow(dead_code)]
#[repr(transparent)]
#[derive(Clone, Debug)]
pub struct Mat4<T>([Vec4<T>; 4]);

impl<T> Mat4<T>
where
    T: Copy,
{
    #[inline]
    pub const fn from_rows(r0: Vec4<T>, r1: Vec4<T>, r2: Vec4<T>, r3: Vec4<T>) -> Self {
        Self([
            Vec4::new(r0.x(), r1.x(), r2.x(), r3.x()),
            Vec4::new(r0.y(), r1.y(), r2.y(), r3.y()),
            Vec4::new(r0.z(), r1.z(), r2.z(), r3.z()),
            Vec4::new(r0.w(), r1.w(), r2.w(), r3.w()),
        ])
    }
}

impl<T> Mat4<T> {
    #[inline]
    pub const fn from_cols(c0: Vec4<T>, c1: Vec4<T>, c2: Vec4<T>, c3: Vec4<T>) -> Self {
        Self([c0, c1, c2, c3])
    }
}

impl<T: One + Zero + Copy> Mat4<T> {
    pub const fn scaling(s: Vec4<T>) -> Self {
        Self::from_cols(
            Vec4::new(s.x(), T::ZERO, T::ZERO, T::ZERO),
            Vec4::new(T::ZERO, s.y(), T::ZERO, T::ZERO),
            Vec4::new(T::ZERO, T::ZERO, s.z(), T::ZERO),
            Vec4::new(T::ZERO, T::ZERO, T::ZERO, s.w()),
        )
    }

    pub const fn translation(t: Vec3<T>) -> Self {
        Self::from_cols(
            Vec4::new(T::ONE, T::ZERO, T::ZERO, T::ZERO),
            Vec4::new(T::ZERO, T::ONE, T::ZERO, T::ZERO),
            Vec4::new(T::ZERO, T::ZERO, T::ONE, T::ZERO),
            Vec4::new(t.x(), t.y(), t.z(), T::ONE),
        )
    }
}

impl<T: Zero> Zero for Mat4<T> {
    const ZERO: Self = Self::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO);
}

impl<T: Zero + One> Identity for Mat4<T> {
    const IDENTITY: Self = Self::from_cols(
        Vec4::new(T::ONE, T::ZERO, T::ZERO, T::ZERO),
        Vec4::new(T::ZERO, T::ONE, T::ZERO, T::ZERO),
        Vec4::new(T::ZERO, T::ZERO, T::ONE, T::ZERO),
        Vec4::new(T::ZERO, T::ZERO, T::ZERO, T::ONE),
    );
}

impl<T: Copy> Mat4<T> {
    #[inline]
    pub const fn r0(&self) -> Vec4<T> {
        Vec4::new(self.c0().x(), self.c1().x(), self.c2().x(), self.c3().x())
    }
    #[inline]
    pub const fn r1(&self) -> Vec4<T> {
        Vec4::new(self.c0().y(), self.c1().y(), self.c2().y(), self.c3().y())
    }
    #[inline]
    pub const fn r2(&self) -> Vec4<T> {
        Vec4::new(self.c0().z(), self.c1().z(), self.c2().z(), self.c3().z())
    }
    #[inline]
    pub const fn r3(&self) -> Vec4<T> {
        Vec4::new(self.c0().w(), self.c1().w(), self.c2().w(), self.c3().w())
    }
    #[inline]
    pub const fn c0(&self) -> Vec4<T> {
        self.0[0]
    }
    #[inline]
    pub const fn c1(&self) -> Vec4<T> {
        self.0[1]
    }
    #[inline]
    pub const fn c2(&self) -> Vec4<T> {
        self.0[2]
    }
    #[inline]
    pub const fn c3(&self) -> Vec4<T> {
        self.0[3]
    }
    #[inline]
    pub fn as_2d_arr(&self) -> [[T; 4]; 4] {
        [
            self.c0().into_arr(),
            self.c1().into_arr(),
            self.c2().into_arr(),
            self.c3().into_arr(),
        ]
    }
    #[inline]
    pub fn into_2d_arr(self) -> [[T; 4]; 4] {
        [
            self.c0().into_arr(),
            self.c1().into_arr(),
            self.c2().into_arr(),
            self.c3().into_arr(),
        ]
    }
    #[inline]
    pub const fn as_mat3(&self) -> Mat3<T> {
        Mat3::from_cols(
            self.c0().into_vec3(),
            self.c1().into_vec3(),
            self.c2().into_vec3(),
        )
    }
}

impl<T> Mat4<T> {
    #[inline]
    pub const fn c0_mut(&mut self) -> &mut Vec4<T> {
        &mut self.0[0]
    }
    #[inline]
    pub const fn c1_mut(&mut self) -> &mut Vec4<T> {
        &mut self.0[1]
    }
    #[inline]
    pub const fn c2_mut(&mut self) -> &mut Vec4<T> {
        &mut self.0[2]
    }
    #[inline]
    pub const fn c3_mut(&mut self) -> &mut Vec4<T> {
        &mut self.0[3]
    }
}

impl Mat4<f32> {
    pub const fn mul(&self, rhs: &Self) -> Mat4<f32> {
        let (r0, r1, r2, r3) = (self.r0(), self.r1(), self.r2(), self.r3());

        Self::from_cols(
            Vec4::new(
                r0.dot(&rhs.c0()),
                r1.dot(&rhs.c0()),
                r2.dot(&rhs.c0()),
                r3.dot(&rhs.c0()),
            ),
            Vec4::new(
                r0.dot(&rhs.c1()),
                r1.dot(&rhs.c1()),
                r2.dot(&rhs.c1()),
                r3.dot(&rhs.c1()),
            ),
            Vec4::new(
                r0.dot(&rhs.c2()),
                r1.dot(&rhs.c2()),
                r2.dot(&rhs.c2()),
                r3.dot(&rhs.c2()),
            ),
            Vec4::new(
                r0.dot(&rhs.c3()),
                r1.dot(&rhs.c3()),
                r2.dot(&rhs.c3()),
                r3.dot(&rhs.c3()),
            ),
        )
    }
}

impl<T: std::fmt::Display + Copy> std::fmt::Display for Mat4<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        write!(f, "{}, ", self.c0())?;
        write!(f, "{}, ", self.c1())?;
        write!(f, "{}, ", self.c2())?;
        write!(f, "{}", self.c3())?;
        write!(f, "]")
    }
}

impl<T: PartialEq + Copy> PartialEq for Mat4<T> {
    fn eq(&self, other: &Self) -> bool {
        self.c0() == other.c0()
            && self.c1() == other.c1()
            && self.c2() == other.c2()
            && self.c3() == other.c3()
    }
    fn ne(&self, other: &Self) -> bool {
        self.c0() != other.c0()
            || self.c1() != other.c1()
            || self.c2() != other.c2()
            || self.c3() != other.c3()
    }
}

#[cfg(test)]
mod test {
    use crate::mat4::Mat4;
    use crate::vec4::Vec4;

    #[test]
    fn multiplication_scaling() {
        let s = Mat4::scaling(Vec4::new(2.0, 3.0, 4.0, 5.0));

        let m = Mat4::from_cols(
            Vec4::new(1.0, 2.0, 3.0, 4.0),
            Vec4::new(5.0, 6.0, 7.0, 8.0),
            Vec4::new(9.0, 10.0, 11.0, 12.0),
            Vec4::new(13.0, 14.0, 15.0, 16.0),
        );

        let result = m.mul(&s);

        let expected = Mat4::from_cols(
            Vec4::new(2.0, 4.0, 6.0, 8.0),
            Vec4::new(15.0, 18.0, 21.0, 24.0),
            Vec4::new(36.0, 40.0, 44.0, 48.0),
            Vec4::new(65.0, 70.0, 75.0, 80.0),
        );

        assert_eq!(result, expected);
    }

    #[test]
    fn multiplication_chained() {
        let a = Mat4::from_rows(
            Vec4::new(1.0, 2.0, 3.0, 4.0),
            Vec4::new(5.0, 6.0, 7.0, 8.0),
            Vec4::new(9.0, 10.0, 11.0, 12.0),
            Vec4::new(13.0, 14.0, 15.0, 16.0),
        );

        let b = Mat4::from_rows(
            Vec4::new(17.0, 18.0, 19.0, 20.0),
            Vec4::new(21.0, 22.0, 23.0, 24.0),
            Vec4::new(25.0, 26.0, 27.0, 28.0),
            Vec4::new(29.0, 30.0, 31.0, 32.0),
        );

        let c = Mat4::from_rows(
            Vec4::new(33.0, 34.0, 35.0, 36.0),
            Vec4::new(37.0, 38.0, 39.0, 40.0),
            Vec4::new(41.0, 42.0, 43.0, 44.0),
            Vec4::new(45.0, 46.0, 47.0, 48.0),
        );

        let result = a.mul(&b).mul(&c);

        // expected == a * b * c
        let expected = Mat4::from_rows(
            Vec4::new(41540.0, 42600.0, 43660.0, 44720.0),
            Vec4::new(103012.0, 105640.0, 108268.0, 110896.0),
            Vec4::new(164484.0, 168680.0, 172876.0, 177072.0),
            Vec4::new(225956.0, 231720.0, 237484.0, 243248.0),
        );

        assert_eq!(result, expected);
    }
}
