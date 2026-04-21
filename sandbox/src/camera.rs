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

// #[allow(unused)]
impl Camera {
    pub fn orthographic(width: f32, height: f32, depth: f32) -> Self {
        debug_assert!(width != 0.0);
        debug_assert!(height != 0.0);
        debug_assert!(depth != 0.0);
        
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
        debug_assert!(fov_y > 0.0);
        
        let projection = PerspectiveProjection::new(fov_y);
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
    // #[inline]
    // pub fn view_projection(&self) -> Mat4<f32> {
    //     self.view_matrix().mul(&self.projection_matrix())
    // }
    #[inline]
    pub fn look_at(&mut self, target: Vec3<f32>, up: Vec3<f32>) {
        let f = target.sub(self.transform.position).normalized();    
        let r = f.cross(up);
        let u = r.cross(f);
    
        self.transform.orientation = Quat::from_basis(r, u, Vec3::ZERO.sub(f));
        // self.transform.orientation = Quat::from_basis(r, u, f);
    }
    pub fn set_zoom(&mut self, new_zoom: f32) {
        debug_assert!(new_zoom > 0.0);

        match &mut self.projection {
            Projection::Orthographic(o) => o.zoom = new_zoom,
            Projection::Perspective(p) => p.zoom = new_zoom,
        }
    }
    pub fn get_zoom(&self) -> f32 {
        match &self.projection {
            Projection::Orthographic(o) => o.zoom,
            Projection::Perspective(p) => p.zoom,
        }
    }
    pub fn set_aspect_ratio(&mut self, new_aspect_ratio: f32) {
        debug_assert!(new_aspect_ratio > 0.0);
        
        match &mut self.projection {
            Projection::Orthographic(o) => o.aspect_ratio = new_aspect_ratio,
            Projection::Perspective(p) => p.aspect_ratio = new_aspect_ratio,
        }
    }
}
