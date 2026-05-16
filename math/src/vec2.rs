use crate::traits::Zero;

use crate::vec3::Vec3;
use crate::vec4::Vec4;

#[allow(dead_code)]
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Vec2<T>(pub(crate) [T; 2]);

impl<T> Vec2<T> {
    #[inline]
    pub const fn new(x: T, y: T) -> Self {
        Self([x, y])
    }
}

impl<T: Zero> Zero for Vec2<T> {
    const ZERO: Self = Self::new(T::ZERO, T::ZERO);
}

impl<T: Zero + Copy> Vec2<T> {
    #[inline]
    pub const fn into_vec3(self) -> Vec3<T> {
        Vec3::new(self.x(), self.y(), T::ZERO)
    }
    #[inline]
    pub const fn into_vec4(self) -> Vec4<T> {
        Vec4::new(self.x(), self.y(), T::ZERO, T::ZERO)
    }
}

impl<T> Vec2<T>
where
    T: Copy,
{
    #[inline]
    pub const fn x(&self) -> T {
        self.0[0]
    }
    #[inline]
    pub const fn y(&self) -> T {
        self.0[1]
    }
}

impl<T> Vec2<T> {
    #[inline]
    pub const fn x_mut(&mut self) -> &mut T {
        &mut self.0[0]
    }
    #[inline]
    pub const fn y_mut(&mut self) -> &mut T {
        &mut self.0[1]
    }
}

impl Vec2<f32> {
    #[inline]
    pub const fn length_squared(&self) -> f32 {
        self.x() * self.x() + self.y() * self.y()
    }
    #[inline]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt() // NOTE: sqrt is not const
    }

    #[inline]
    pub const fn scaled(self, s: f32) -> Self {
        Vec2::new(self.x() * s, self.y() * s)
    }
    #[inline]
    pub const fn scale_assign(&mut self, s: f32) {
        *self = self.scaled(s)
    }
    #[inline]
    pub fn normalized(mut self) -> Self {
        let l = self.length(); // NOTE: len is not const

        if l != 0.0 {
            self.scale_assign(1.0 / l);
        }

        self
    }
    #[inline]
    pub const fn add(&self, other: Self) -> Self {
        Self::new(self.x() + other.x(), self.y() + other.y())
    }
    #[inline]
    pub const fn add_assign(&mut self, other: Self) {
        *self.x_mut() += other.x();
        *self.y_mut() += other.y();
    }
    #[inline]
    pub const fn sub(&self, other: Self) -> Self {
        Self::new(self.x() - other.x(), self.y() - other.y())
    }
    #[inline]
    pub const fn sub_assign(&mut self, other: Self) {
        *self.x_mut() -= other.x();
        *self.y_mut() -= other.y();
    }
    #[inline]
    pub const fn dot(&self, other: Self) -> f32 {
        self.x() * other.x() + self.y() * other.y()
    }
}

impl<T: std::fmt::Display + Copy> std::fmt::Display for Vec2<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{x: {}, y: {}}}", self.x(), self.y())
    }
}

impl<T: PartialEq + Copy> PartialEq for Vec2<T> {
    fn eq(&self, other: &Self) -> bool {
        self.x() == other.x() && self.y() == other.y()
    }
    fn ne(&self, other: &Self) -> bool {
        self.x() != other.x() || self.y() != other.y()
    }
}

#[cfg(test)]
mod tests {
    use crate::Vec2;

    #[test]
    fn add1() {
        let mut a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(4.0, 6.0);
        let c = Vec2::new(5.0, 8.0);

        assert_eq!(a.add(b), c);
        a.add_assign(b);
        assert_eq!(a, c);
    }

    #[test]
    fn sub1() {
        let mut c = Vec2::<f32>::new(5.0, 8.0);
        let b = Vec2::<f32>::new(4.0, 6.0);
        let a = Vec2::<f32>::new(1.0, 2.0);

        assert_eq!(c.sub(b), a);
        c.sub_assign(b);
        assert_eq!(c, a);
    }

    #[test]
    fn scale1() {
        let mut v = Vec2::<f32>::new(13.0, 27.0);
        let s1 = 0.5;
        let v1 = Vec2::<f32>::new(6.5, 13.5);

        assert_eq!(v.scaled(s1), v1);
        v.scale_assign(s1);
        assert_eq!(v, v1);
    }

    #[test]
    fn dot1() {
        let a = Vec2::<f32>::new(1.0, 5.0);
        let b = Vec2::<f32>::new(17.0, 65.0);
        let c: f32 = 342.0;

        assert_eq!(a.dot(b), c);
    }

    #[test]
    fn normalize1() {
        let a = Vec2::<f32>::new(44.0, 55.0);
        let b = Vec2::<f32>::new(0.62469506, 0.7808688);

        assert_eq!(a.normalized(), b);
    }
}
