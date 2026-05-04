use crate::{Vec3, traits::Zero};

#[allow(dead_code)]
#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Vec4<T>(pub(crate) [T; 4]);

impl<T> Vec4<T> {
    #[inline]
    pub const fn new(a: T, b: T, c: T, d: T) -> Self {
        Self([a, b, c, d])
    }
}

impl<T> Vec4<T>
where
    T: Copy,
{
    #[inline]
    pub const fn from_vec3(v: crate::vec3::Vec3<T>, d: T) -> Self {
        Self::new(v.x(), v.y(), v.z(), d)
    }
}

impl<T: Zero> Zero for Vec4<T> {
    const ZERO: Self = Self::new(T::ZERO, T::ZERO, T::ZERO, T::ZERO);
}

impl<T> Vec4<T>
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
    #[inline]
    pub const fn z(&self) -> T {
        self.0[2]
    }
    #[inline]
    pub const fn w(&self) -> T {
        self.0[3]
    }
    #[inline]
    pub const fn as_arr(&self) -> [T; 4] {
        self.0
    }
    #[inline]
    pub const fn into_vec3(self) -> Vec3<T> {
        Vec3::new(self.x(), self.y(), self.z())
    }
}

impl<T> Vec4<T> {
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
    #[inline]
    pub const fn w_mut(&mut self) -> &mut T {
        &mut self.0[3]
    }
    #[inline]
    pub fn into_arr(self) -> [T; 4] {
        self.0
    }
}

impl Vec4<f32> {
    #[inline]
    pub const fn len_squared(&self) -> f32 {
        let x = self.x();
        let y = self.y();
        let z = self.z();
        let w = self.w();

        x * x + y * y + z * z + w * w
    }
    #[inline]
    pub fn len(&self) -> f32 {
        self.len_squared().sqrt() // NOTE: sqrt is not const
    }

    #[inline]
    pub const fn scaled(&self, s: f32) -> Self {
        Self::new(self.x() * s, self.y() * s, self.z() * s, self.w() * s)
    }
    #[inline]
    pub const fn scale_assign(&mut self, s: f32) {
        *self = self.scaled(s)
    }
    #[inline]
    pub const fn scaled_nonuniform(&self, s: Self) -> Self {
        Self::new(
            self.x() * s.x(),
            self.y() * s.y(),
            self.z() * s.z(),
            self.w() * s.w(),
        )
    }
    #[inline]
    pub const fn scale_assign_nonuniform(&mut self, s: Self) {
        *self = self.scaled_nonuniform(s)
    }
    #[inline]
    pub fn normalized(mut self) -> Self {
        let l = self.len(); // NOTE: len is not const

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
            self.w() + other.w(),
        )
    }
    #[inline]
    pub const fn add_assign(&mut self, other: Self) {
        *self.x_mut() += other.x();
        *self.y_mut() += other.y();
        *self.z_mut() += other.z();
        *self.w_mut() += other.w();
    }
    #[inline]
    pub const fn sub(&self, other: Self) -> Self {
        Self::new(
            self.x() - other.x(),
            self.y() - other.y(),
            self.z() - other.z(),
            self.w() - other.w(),
        )
    }
    #[inline]
    pub const fn sub_assign(&mut self, other: Self) {
        *self.x_mut() -= other.x();
        *self.y_mut() -= other.y();
        *self.z_mut() -= other.z();
        *self.w_mut() -= other.w();
    }
    #[inline]
    pub const fn dot(&self, other: &Self) -> f32 {
        self.x() * other.x() + self.y() * other.y() + self.z() * other.z() + self.w() * other.w()
    }
}

impl<T: std::fmt::Display + Copy> std::fmt::Display for Vec4<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{{x: {}, y: {}, z: {}, w: {}}}",
            self.x(),
            self.y(),
            self.z(),
            self.w()
        )
    }
}

impl<T: PartialEq + Copy> PartialEq for Vec4<T> {
    fn eq(&self, other: &Self) -> bool {
        self.x() == other.x()
            && self.y() == other.y()
            && self.z() == other.z()
            && self.w() == other.w()
    }
    fn ne(&self, other: &Self) -> bool {
        self.x() != other.x()
            || self.y() != other.y()
            || self.z() != other.z()
            || self.w() != other.w()
    }
}

#[cfg(test)]
mod tests {
    use crate::vec4::Vec4;

    #[test]
    fn add1() {
        let mut a = Vec4::<f32>::new(1.0, 5.0, 9.0, 17.0);
        let b = Vec4::<f32>::new(33.0, 65.0, 125.0, 257.0);
        let c = Vec4::<f32>::new(34.0, 70.0, 134.0, 274.0);

        assert_eq!(a.add(b), c);
        a.add_assign(b);
        assert_eq!(a, c);
    }

    #[test]
    fn sub1() {
        let a = Vec4::<f32>::new(1.0, 5.0, 9.0, 17.0);
        let b = Vec4::<f32>::new(33.0, 65.0, 125.0, 257.0);
        let mut c = Vec4::<f32>::new(34.0, 70.0, 134.0, 274.0);

        assert_eq!(c.sub(b), a);
        c.sub_assign(b);
        assert_eq!(c, a);
    }
    #[test]
    fn scale1() {
        let mut v = Vec4::<f32>::new(1.0, 9.0, 33.0, 125.0);
        let s1 = 0.5;
        let v1 = Vec4::<f32>::new(0.5, 4.5, 16.5, 62.5);

        assert_eq!(v.scaled(s1), v1);
        v.scale_assign(s1);
        assert_eq!(v, v1);

        let mut v = Vec4::<f32>::new(1.0, 9.0, 33.0, 125.0);
        let s2 = Vec4::<f32>::new(5.0, 9.0, 17.0, 33.0);
        let v2 = Vec4::<f32>::new(5.0, 81.0, 561.0, 4125.0);

        assert_eq!(v.scaled_nonuniform(s2), v2);
        v.scale_assign_nonuniform(s2);
        assert_eq!(v, v2);
    }
    #[test]
    fn dot1() {
        let a = Vec4::<f32>::new(1.0, 5.0, 9.0, 17.0);
        let b = Vec4::<f32>::new(33.0, 65.0, 125.0, 257.0);
        let c: f32 = 5852.0;

        assert_eq!(a.dot(&b), c);
    }
    #[test]
    fn normalize1() {
        let a = Vec4::<f32>::new(44.0, 55.0, 66.0, 77.0);
        let b = Vec4::<f32>::new(0.35634834, 0.4454354, 0.53452253, 0.6236096);

        assert_eq!(a.normalized(), b);
    }
}
