use crate::traits::Zero;
use crate::vec4::Vec4;

#[allow(dead_code)]
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Vec3<T>(pub(crate) [T; 3]);

impl<T> Vec3<T> {
    #[inline]
    pub const fn new(a: T, b: T, c: T) -> Self {
        Self([a, b, c])
    }
}

impl<T: Zero> Zero for Vec3<T> {
    const ZERO: Self = Self::new(T::ZERO, T::ZERO, T::ZERO);
}

impl<T: Copy> Vec3<T> {
    #[inline]
    pub const fn x(&self) -> T {
        self.0[0]
    }
    #[inline]
    pub const fn y(&self) -> T {
        self.0[1]
    }
    #[inline]
    pub const fn z(&self) -> T {
        self.0[2]
    }
}

impl<T> Vec3<T> {
    #[inline]
    pub const fn x_mut(&mut self) -> &mut T {
        &mut self.0[0]
    }
    #[inline]
    pub const fn y_mut(&mut self) -> &mut T {
        &mut self.0[1]
    }
    #[inline]
    pub const fn z_mut(&mut self) -> &mut T {
        &mut self.0[2]
    }
}

impl<T: Zero + Copy> Vec3<T> {
    #[inline]
    pub const fn into_vec4(self) -> Vec4<T> {
        Vec4::new(self.x(), self.y(), self.z(), T::ZERO)
    }
    #[inline]
    pub const fn as_vec4(&self) -> Vec4<T> {
        Vec4::new(self.x(), self.y(), self.z(), T::ZERO)
    }
    #[inline]
    pub const fn into_arr(self) -> [T; 3] {
        [self.x(), self.y(), self.z()]
    }
    #[inline]
    pub const fn as_arr(&self) -> [T; 3] {
        [self.x(), self.y(), self.z()]
    }
}

impl Vec3<f32> {
    #[inline]
    pub const fn length_squared(&self) -> f32 {
        let x = self.x();
        let y = self.y();
        let z = self.z();
        x * x + y * y + z * z
    }
    #[inline]
    pub fn length(&self) -> f32 {
        self.length_squared().sqrt() // NOTE: sqrt is not const
    }
    #[inline]
    pub const fn scaled(&self, s: f32) -> Self {
        Vec3::new(self.x() * s, self.y() * s, self.z() * s)
    }
    #[inline]
    pub const fn scale_assign(&mut self, s: f32) {
        *self = self.scaled(s)
    }
    #[inline]
    pub const fn scaled_nonuniform(self, s: Vec3<f32>) -> Self {
        Vec3::new(self.x() * s.x(), self.y() * s.y(), self.z() * s.z())
    }
    #[inline]
    pub const fn scale_assign_nonuniform(&mut self, s: Vec3<f32>) {
        *self = self.scaled_nonuniform(s)
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
        Self::new(
            self.x() + other.x(),
            self.y() + other.y(),
            self.z() + other.z(),
        )
    }
    #[inline]
    pub const fn add_assign(&mut self, other: Self) {
        *self.x_mut() += other.x();
        *self.y_mut() += other.y();
        *self.z_mut() += other.z();
    }
    #[inline]
    pub const fn sub(&self, other: Self) -> Self {
        Self::new(
            self.x() - other.x(),
            self.y() - other.y(),
            self.z() - other.z(),
        )
    }
    #[inline]
    pub const fn sub_assign(&mut self, other: Self) {
        *self.x_mut() -= other.x();
        *self.y_mut() -= other.y();
        *self.z_mut() -= other.z();
    }
    #[inline]
    pub const fn dot(&self, other: Self) -> f32 {
        self.x() * other.x() + self.y() * other.y() + self.z() * other.z()
    }
    #[inline]
    pub const fn cross(&self, other: Self) -> Self {
        Self::new(
            self.y() * other.z() - self.z() * other.y(),
            self.z() * other.x() - self.x() * other.z(),
            self.x() * other.y() - self.y() * other.x(),
        )
    }
}

impl<T: std::fmt::Display + Copy> std::fmt::Display for Vec3<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{x: {}, y: {}, z: {}}}", self.x(), self.y(), self.z())
    }
}

impl<T: PartialEq + Copy> PartialEq for Vec3<T> {
    fn eq(&self, other: &Self) -> bool {
        self.x() == other.x() && self.y() == other.y() && self.z() == other.z()
    }
    fn ne(&self, other: &Self) -> bool {
        self.x() != other.x() || self.y() != other.y() || self.z() != other.z()
    }
}

#[cfg(test)]
mod tests {
    use crate::vec3::Vec3;

    #[test]
    fn add1() {
        let mut a = Vec3::<f32>::new(1.0, 5.0, 9.0);
        let b = Vec3::<f32>::new(17.0, 33.0, 65.0);
        let c = Vec3::<f32>::new(18.0, 38.0, 74.0);

        assert_eq!(a.add(b), c);
        a.add_assign(b);
        assert_eq!(a, c);
    }

    #[test]
    fn sub1() {
        let a = Vec3::<f32>::new(1.0, 5.0, 9.0);
        let b = Vec3::<f32>::new(17.0, 33.0, 65.0);
        let mut c = Vec3::<f32>::new(18.0, 38.0, 74.0);

        assert_eq!(c.sub(b), a);
        c.sub_assign(b);
        assert_eq!(c, a);
    }
    #[test]
    fn scale1() {
        let mut v = Vec3::<f32>::new(1.0, 17.0, 65.0);
        let s1 = 0.5;
        let v1 = Vec3::<f32>::new(0.5, 8.5, 32.5);

        assert_eq!(v.scaled(s1), v1);
        v.scale_assign(s1);
        assert_eq!(v, v1);

        let mut v = Vec3::<f32>::new(1.0, 17.0, 65.0);
        let s2 = Vec3::<f32>::new(3.0, 7.0, 9.0);
        let v2 = Vec3::<f32>::new(3.0, 119.0, 585.0);

        assert_eq!(v.scaled_nonuniform(s2), v2);
        v.scale_assign_nonuniform(s2);
        assert_eq!(v, v2);
    }
    #[test]
    fn dot1() {
        let a = Vec3::<f32>::new(1.0, 5.0, 9.0);
        let b = Vec3::<f32>::new(17.0, 33.0, 65.0);
        let c: f32 = 767.0;

        assert_eq!(a.dot(b), c);
    }
    #[test]
    fn cross1() {
        let a = Vec3::<f32>::new(1.0, 5.0, 9.0);
        let b = Vec3::<f32>::new(17.0, 33.0, 65.0);
        let c = Vec3::<f32>::new(28.0, 88.0, -52.0);

        assert_eq!(a.cross(b), c);
    }
    #[test]
    fn cross2() {
        let a = Vec3::<f32>::new(1.0, 5.0, 9.0);
        let b = Vec3::<f32>::new(17.0, 33.0, 65.0);
        let c = Vec3::<f32>::new(125.0, 257.0, 513.0);

        // d == a x b x c
        let d = Vec3::<f32>::new(58508.0, -20864.0, -3804.0);

        assert_eq!(a.cross(b).cross(c), d);
    }
    #[test]
    fn normalize1() {
        let a = Vec3::<f32>::new(44.0, 55.0, 66.0);
        let b = Vec3::<f32>::new(0.45584232, 0.5698029, 0.6837635);

        assert_eq!(a.normalized(), b);
    }
}
