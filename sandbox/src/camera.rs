use core::f32;

use math::*;

use crate::{camera::projections::{OrthographicProjection, PerspectiveProjection}};

pub mod controllers;
pub mod projections;

#[allow(unused)]
#[derive(Debug)]
pub enum Projection {
    Orthographic(projections::OrthographicProjection),
    Perspective(projections::PerspectiveProjection),
}

#[derive(Debug)]
pub struct Camera {
    pub projection: Projection,
    pub transform: RigidTransform,
}

#[allow(unused)]
impl Camera {
    pub fn orthographic(width: f32, height: f32, depth: f32) -> Self {
        let projection = OrthographicProjection::new(width, height, depth);
        let projection = Projection::Orthographic(projection);
        let transform = RigidTransform {
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
        };
        Self {
            projection,
            transform,
        }
    }
    pub fn perspective(fov_y: f32) -> Self {
        let projection = PerspectiveProjection::new(fov_y, 1.0);
        let projection = Projection::Perspective(projection);
        let transform = math::RigidTransform {
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
        };
        Self {
            projection,
            transform,
        }
    }
    #[inline]
    pub fn projection_matrix(&self) -> Mat4<f32> {
        match &self.projection {
            Projection::Orthographic(p) => p.projection_matrix(),
            Projection::Perspective(p) => p.projection_matrix(),
        }
    }
    #[inline]
    pub fn view_matrix(&self) -> Mat4<f32> {
        self.transform.inv().into_mat4()
    }
    #[inline]
    pub fn view_projection(&self) -> Mat4<f32> {
        self.view_matrix().mul(&self.projection_matrix())
    }
    #[inline]
    pub fn look_at(&mut self, target: Vec3<f32>, up: Vec3<f32>) {
        let f = target.sub(self.transform.position).normalized();    
        let r = f.cross(up);
        let u = r.cross(f);
    
        self.transform.orientation = Quat::from_basis(r, u, Vec3::ZERO.sub(f));
        // self.transform.orientation = Quat::from_basis(r, u, f);
    }
    pub fn update_aspect_ratio(&mut self, new_aspect_ratio: f32) {
        match &mut self.projection {
            Projection::Orthographic(o) => o.update_aspect_ratio(new_aspect_ratio),
            Projection::Perspective(p) => p.update_aspect_ratio(new_aspect_ratio),
        }
    }
}
