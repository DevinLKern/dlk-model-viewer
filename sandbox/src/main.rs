mod camera;
mod constants;
mod input_manager;
mod result;
mod settings;

use camera::{Camera, controllers::*};
use constants::{WORLD_FORWARDS, WORLD_RIGHT, WORLD_UP};
use image::DynamicImage;
use input_manager::{Input, InputEvent, InputManager};
use renderer::{MaterialUBO, ShaderVertVertex};
use result::{Error, Result};
use settings::{Command, Event, Settings};

use ash::vk;

use std::collections::HashSet;
use std::str::FromStr;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowId},
};

use math::Identity;
use math::Vec3;
use math::Vec4;
use math::{Quat, Zero};

#[derive(Debug)]
enum CameraInUse {
    Fps,
    Orbit,
}

struct Application {
    last: std::time::Instant,
    window_name: Box<str>,
    settings: Settings,
    binding_map: HashMap<Command, usize>,
    input_manager: InputManager,
    toggled: HashSet<Input>,
    camera_in_use: CameraInUse,
    fps_camera: Camera,
    fps_controller: FpsCameraController,
    orbit_camera: Camera,
    orbit_controller: OrbitCameraController,
    windows: HashMap<WindowId, (renderer::RenderContext, Window)>,
    renderer: renderer::Renderer,
    draw_infos: Box<[(vulkan::VertexBV, vulkan::IndexBV, u32)]>,
    _model_transform: math::AffineTransform,
    global_light_direction: Vec3<f32>,
    global_light_color: Vec4<f32>,
    global_ambient_light: f32,
    exiting: bool,
}

const DEFAULT_IMAGE: &[u8] = include_bytes!("../../files/images/default.png");

impl Application {
    fn search_for(base: &Path, target: &Path) -> Option<PathBuf> {
        if !base.is_dir() {
            return None;
        }

        let mut ancestors = base.ancestors();
        while let Some(ancestor) = ancestors.next() {
            let cur = ancestor.join(target);

            if cur.exists() {
                return Some(cur);
            }
        }

        return None;
    }
    fn new(
        window_name: Box<str>,
        settings: crate::Settings,
        model_path: &std::path::Path,
        debug_enabled: bool,
        display_handle: &winit::raw_window_handle::DisplayHandle,
    ) -> Result<Self> {
        // load materials
        let file_path = model_path.with_extension("mtl");
        let mtl_materials = obj_mtl::load_materials(&file_path)?;

        // load textures and images
        let (texture_data, texture_name_to_index) = {
            let mut texture_data = Vec::<DynamicImage>::with_capacity(8);
            let mut texture_indices = HashMap::<Box<str>, usize>::new();
            let default_texture_data =
                image::load_from_memory_with_format(DEFAULT_IMAGE, image::ImageFormat::Png)?;

            texture_data.push(default_texture_data);

            // load diffuse textures
            for mat in mtl_materials.iter() {
                let diffuse_texture = if let Some(texture) = &mat.diffuse.texture {
                    texture
                } else {
                    continue;
                };

                // skip if texture already loaded
                if texture_indices.contains_key(&diffuse_texture.file_path) {
                    continue;
                }

                // search for texture
                let path = {
                    let base = model_path.with_file_name("");
                    let target = match PathBuf::from_str(&diffuse_texture.file_path) {
                        Ok(path) => path,
                        Err(_) => {
                            tracing::warn!(
                                "Malformed diffuse texture file path. Reverting to base color."
                            );
                            continue;
                        }
                    };

                    match Self::search_for(&base, &target) {
                        Some(path) => path,
                        None => {
                            tracing::warn!(
                                "Could not find diffuse texture. Reverting to base color."
                            );
                            continue;
                        }
                    }
                };

                let data = image::open(&path)?;

                let index = texture_data.len();
                texture_indices.insert(diffuse_texture.file_path.clone(), index);
                texture_data.push(data);
            }

            (texture_data, texture_indices)
        };

        // create materials
        let mut materials = Vec::<renderer::MaterialUBO>::with_capacity(mtl_materials.len() + 1);
        // add default material
        materials.push(renderer::MaterialUBO {
            flags: 0,
            texture_index: 0,
            _pad2: [0; 8],
            base_color: [0.8, 0.2, 0.2, 1.0],
        });
        let mut name_to_material_index = HashMap::<Box<str>, usize>::new();
        for material in mtl_materials.into_iter() {
            if name_to_material_index.contains_key(&material.name) {
                continue;
            }

            let (color, texture) = (
                material.diffuse.color.as_ref(),
                material.diffuse.texture.as_ref(),
            );
            let (color, texture, flags) = match (color, texture) {
                (Some(c), Some(t)) => {
                    if let Some(&idx) = texture_name_to_index.get(&t.file_path) {
                        ([c[0], c[1], c[2], 1.0], idx as u32, 1u32)
                    } else {
                        tracing::warn!(
                            "Texture '{}' not found in loaded textures. Falling back to base color.",
                            t.file_path
                        );
                        ([c[0], c[1], c[2], 1.0], 0u32, 0u32)
                    }
                }
                (Some(c), None) => ([c[0], c[1], c[2], 1.0], 0u32, 0u32),
                (None, Some(t)) => {
                    if let Some(&idx) = texture_name_to_index.get(&t.file_path) {
                        ([0.0, 0.0, 0.0, 0.0], idx as u32, 1u32)
                    } else {
                        tracing::warn!(
                            "Texture '{}' not found in loaded textures. Disabling texture flag.",
                            t.file_path
                        );
                        ([1.0, 1.0, 1.0, 1.0], 0u32, 0u32)
                    }
                }
                (None, None) => ([1.0, 1.0, 1.0, 1.0], 0u32, 0u32),
            };

            let idx = materials.len();
            materials.push(MaterialUBO {
                flags,
                base_color: color,
                texture_index: texture,
                _pad2: [0; 8],
            });
            name_to_material_index.insert(material.name, idx);
        }

        let (model_transform, plane_transform, mesh_data) = {
            // vertex_data, index_data, material_index, model_transform_index
            let mut mesh_data = Vec::<(Vec<ShaderVertVertex>, Vec<u32>, u32)>::new();

            let plane_transform = math::AffineTransform {
                position: Vec3::ZERO.sub(WORLD_UP).scaled(0.5),
                orientation: Quat::IDENTITY,
                scalar: Vec3::new(1000.0, 1000.0, 1000.0),
            };

            let plane_vertex_buffer_data = {
                const F: Vec3<f32> = WORLD_FORWARDS;
                const B: Vec3<f32> = Vec3::ZERO.sub(WORLD_FORWARDS);
                const R: Vec3<f32> = WORLD_RIGHT;
                const L: Vec3<f32> = Vec3::ZERO.sub(WORLD_RIGHT);

                const FR: Vec3<f32> = F.add(R);
                const FL: Vec3<f32> = F.add(L);
                const BR: Vec3<f32> = B.add(R);
                const BL: Vec3<f32> = B.add(L);

                vec![
                    renderer::ShaderVertVertex {
                        position: FL.into_arr(),
                        tex_coord: [1.0, 0.0],
                        normal: WORLD_UP.into_arr(),
                    },
                    renderer::ShaderVertVertex {
                        position: FR.into_arr(),
                        tex_coord: [0.0, 0.0],
                        normal: WORLD_UP.into_arr(),
                    },
                    renderer::ShaderVertVertex {
                        position: BR.into_arr(),
                        tex_coord: [0.0, 1.0],
                        normal: WORLD_UP.into_arr(),
                    },
                    renderer::ShaderVertVertex {
                        position: BL.into_arr(),
                        tex_coord: [1.0, 1.0],
                        normal: WORLD_UP.into_arr(),
                    },
                ]
            };
            let plane_index_buffer_data = vec![0, 1, 2, 2, 3, 0];

            // 0 is the index of the default material
            mesh_data.push((plane_vertex_buffer_data, plane_index_buffer_data, 0));

            use obj_mtl::*;
            let objf = ObjScene::from_file(model_path)?;
            for shape in objf.get_shapes() {
                let mut vertices = Vec::<ShaderVertVertex>::new();
                let mut indices = Vec::<u32>::new();
                let mut vertex_map = HashMap::<VtnIndex, u32>::new();

                let material_idx: u32 = if shape.materials.len() > 0 {
                    if shape.materials.len() != 1 {
                        tracing::warn!("Multiple materials per shape not supported");
                    }

                    let mat = shape.materials.first().unwrap();
                    let idx = name_to_material_index.get(mat).unwrap_or_else(|| {
                        tracing::warn!("Could not find {} material. Defaulting to 0", mat);
                        &0
                    });
                    *idx as u32
                } else {
                    0
                };

                // Build a triangle list (fan triangulation for polygons/quads).
                let mut triangles = Vec::<(VtnIndex, VtnIndex, VtnIndex)>::with_capacity(64);
                for primitive in shape.get_primitives() {
                    match primitive {
                        Primitive::Triangle { v0, v1, v2 } => triangles.push((*v0, *v1, *v2)),
                        Primitive::Polygon(poly) => {
                            if poly.len() < 3 {
                                continue;
                            }
                            let v0 = poly[0];
                            for i in 1..(poly.len() - 1) {
                                triangles.push((v0, poly[i], poly[i + 1]));
                            }
                        }
                        _ => {}
                    }
                }

                for (v0, v1, v2) in triangles {
                    let derived_normal = if settings.derive_normals {
                        match (v0.vn, v1.vn, v2.vn) {
                            (None, None, None) => {
                                let p0 = &objf.vs[v0.v];
                                let p0 = Vec3::new(p0.x as f32, p0.y as f32, p0.z as f32);
                                let p1 = &objf.vs[v1.v];
                                let p1 = Vec3::new(p1.x as f32, p1.y as f32, p1.z as f32);
                                let p2 = &objf.vs[v2.v];
                                let p2 = Vec3::new(p2.x as f32, p2.y as f32, p2.z as f32);

                                let face_normal = p1.sub(p0).cross(p2.sub(p0));

                                Some(face_normal.into_arr())
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    let derived_normal = match derived_normal {
                        Some(n) => n,
                        None => [0.0, 0.0, 0.0],
                    };

                    for v in [v0, v1, v2] {
                        let index = if let Some(&i) = vertex_map.get(&v) {
                            i
                        } else {
                            let position = [
                                objf.vs[v.v].x as f32,
                                objf.vs[v.v].y as f32,
                                objf.vs[v.v].z as f32,
                            ];

                            let tex_coord = if let Some(i) = v.vt {
                                [objf.vts[i].u as f32, 1.0 - objf.vts[i].v as f32]
                            } else {
                                [0.0, 0.0]
                            };

                            let normal = if let Some(i) = v.vn {
                                [
                                    objf.vns[i].x as f32,
                                    objf.vns[i].y as f32,
                                    objf.vns[i].z as f32,
                                ]
                            } else {
                                derived_normal
                            };

                            let i = vertices.len() as u32;
                            vertices.push(ShaderVertVertex {
                                position,
                                tex_coord,
                                normal,
                            });
                            vertex_map.insert(v, i);
                            i
                        };

                        indices.push(index);
                    }
                }

                mesh_data.push((vertices, indices, material_idx));
            }

            let model_transform = {
                let mut min = [f32::MAX; 3];
                let mut max = [f32::MIN; 3];
                for (vertices, _, _) in mesh_data.iter() {
                    for v in vertices.iter() {
                        for i in 0..3 {
                            min[i] = min[i].min(v.position[i]);
                            max[i] = max[i].max(v.position[i]);
                        }
                    }
                }
                let min = Vec3::new(min[0], min[1], min[2]);
                let max = Vec3::new(max[0], max[1], max[2]);

                let model_scale = (max.x() - min.x())
                    .max(max.y() - min.y())
                    .max(max.z() - min.z());
                let model_scale = 1.0 / model_scale;

                let model_pos = {
                    let min_normalized = min.scaled(model_scale);
                    let min_reduced = min_normalized.scaled_nonuniform(WORLD_UP);
                    let plane_reduced = plane_transform.position.scaled_nonuniform(WORLD_UP);

                    plane_reduced.sub(min_reduced)
                };

                math::AffineTransform {
                    position: model_pos,
                    orientation: Quat::IDENTITY,
                    scalar: Vec3::new(model_scale, model_scale, model_scale),
                }
            };

            (model_transform, plane_transform, mesh_data)
        };

        let mut mesh_ubo_buffer_data: Box<[(math::AffineTransform, u32)]> = mesh_data
            .iter()
            .map(|(_, _, material_index)| (model_transform, *material_index))
            .collect();
        mesh_ubo_buffer_data[0] = (plane_transform, 0);

        let renderer = renderer::Renderer::new(
            debug_enabled,
            display_handle,
            mesh_ubo_buffer_data.len() as u64,
            &texture_data,
            &materials,
        )?;

        let mut draw_infos = Vec::<(vulkan::VertexBV, vulkan::IndexBV, u32)>::new();
        for (vb_data, ib_data, mesh_idx) in mesh_data.into_iter() {
            if vb_data.len() == 0 || ib_data.len() == 0 {
                continue;
            }
            let vb_data_u8 = unsafe {
                std::slice::from_raw_parts(
                    vb_data.as_ptr() as *const u8,
                    vb_data.len() * std::mem::size_of::<renderer::ShaderVertVertex>(),
                )
            };

            let vb = renderer.create_vertex_buffer(&vb_data_u8, vb_data.len() as u32)?;

            let ib_data_u8 = unsafe {
                std::slice::from_raw_parts(
                    ib_data.as_ptr() as *const u8,
                    ib_data.len() * std::mem::size_of::<u32>(),
                )
            };

            let ib = renderer.create_index_buffer(
                ib_data_u8,
                vk::IndexType::UINT32,
                ib_data.len() as u32,
                0,
            )?;

            draw_infos.push((vb, ib, mesh_idx))
        }

        for (i, (transform, material_index)) in mesh_ubo_buffer_data.iter().enumerate() {
            let model_transform = if i != 0 {
                transform
                    .as_mat4()
                    .mul(&settings.model_to_world.as_mat4(1.0))
            } else {
                transform.as_mat4()
            };
            let ubo_data = renderer::MeshUBO {
                model: model_transform.into_2d_arr(),
                material_index: *material_index,
            };
            let src = &ubo_data;
            let offset = i as u64 * renderer.model_transform_buffer_element_size;
            unsafe {
                let dst = renderer
                    .model_transform_buffer
                    .map_memory(offset, renderer.model_transform_buffer_element_size)
                    .inspect_err(|e| tracing::error!("{e}"))
                    .unwrap();

                std::ptr::copy_nonoverlapping(src, dst as *mut renderer::MeshUBO, 1);

                renderer.model_transform_buffer.unmap()
            }
        }

        let mut binding_map = HashMap::new();
        for (index, binding) in settings.bindings.iter().enumerate() {
            binding_map.insert(binding.command, index);
        }

        // causes blank screen
        let mut orbit_camera =  Camera::orthographic(1.0, 1.0, 10.0);
        orbit_camera.transform.translate_global(Vec3::ZERO.sub(WORLD_FORWARDS.scaled(2.0)));
        orbit_camera.look_at(model_transform.position, WORLD_UP);

        let mut fps_camera = Camera::perspective(settings.fov_y);
        fps_camera.transform.translate_global(Vec3::ZERO.sub(WORLD_FORWARDS));
        fps_camera.look_at(model_transform.position, WORLD_UP);

        let last = std::time::Instant::now();

        Ok(Self {
            last,
            window_name,
            settings,
            binding_map,
            input_manager: InputManager::new(),
            toggled: HashSet::<Input>::new(),
            renderer,
            camera_in_use: CameraInUse::Orbit,
            fps_camera,
            fps_controller: FpsCameraController::new(),
            orbit_camera,
            orbit_controller: OrbitCameraController::new(model_transform.position),
            windows: std::collections::HashMap::new(),
            draw_infos: draw_infos.into_boxed_slice(),
            exiting: false,
            _model_transform: model_transform,
            global_light_direction: Vec3::ZERO.sub(WORLD_UP).add(WORLD_RIGHT.scaled(0.2)),
            global_light_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            global_ambient_light: 0.1,
        })
    }
    fn meets_requirements(&self, binding_index: usize) -> Option<bool> {
        let binding = self.settings.bindings.get(binding_index)?;
        let b = match binding.event {
            Event::Hold => self.input_manager.is_held(&binding.input),
            Event::Press => self.input_manager.just_pressed(&binding.input),
            Event::Release => self.input_manager.just_released(&binding.input),
            Event::Toggle => self.toggled.contains(&binding.input),
            Event::Movement => {
                self.input_manager.mouse_delta.0 != 0.0 || self.input_manager.mouse_delta.1 != 0.0
            }
        };

        let requirements_met = if let Some(idx) = binding.requirement {
            self.meets_requirements(idx)?
        } else {
            true
        };

        Some(b && requirements_met)
    }
    fn execute_commands(&mut self) {
        let mut offset = Vec3::ZERO;
        let dirs = [
            (Command::MoveForward, WORLD_FORWARDS),
            (Command::MoveBackward, Vec3::ZERO.sub(WORLD_FORWARDS)),
            (Command::MoveRight, WORLD_RIGHT),
            (Command::MoveLeft, Vec3::ZERO.sub(WORLD_RIGHT)),
            (Command::MoveUp, WORLD_UP),
            (Command::MoveDown, Vec3::ZERO.sub(WORLD_UP)),
        ];
        for (cmd, dir) in dirs.iter() {
            let binding_index = match self.binding_map.get(cmd) {
                Some(x) => x,
                _ => continue,
            };

            if self.meets_requirements(*binding_index).unwrap() {
                offset.add_assign(*dir);
            }
        }
        let (dx, dy) = if let Some(idx) = self.binding_map.get(&Command::Rotate) {
            if let Some(true) = self.meets_requirements(*idx) {
                let dx = (self.input_manager.mouse_delta.0) as f32;
                let dy = (self.input_manager.mouse_delta.1) as f32;
                (dx, dy)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };

        // switch from orbit to fps
        if let Some(idx) = self.binding_map.get(&Command::UseFpsCamera) {
            if let Some(true) = self.meets_requirements(*idx) {
                self.camera_in_use = CameraInUse::Fps;
            }
        }

        // switch from fps to orbit
        if let Some(idx) = self.binding_map.get(&Command::UseOrbitCamera) {
            if let Some(true) = self.meets_requirements(*idx) {
                self.camera_in_use = CameraInUse::Orbit;
            }
        }

        let now = std::time::Instant::now();
        let elapsed = (now - self.last).as_secs_f32();
        self.last = now;

        // camera should move at 2 unites per second
        const SPEED: f32 = 2.000;
        match self.camera_in_use {
            CameraInUse::Fps => {
                offset.scale_assign(SPEED);
                self.fps_controller.move_local(offset);
                self.fps_controller.rotate(dx, dy);

                self.fps_controller
                    .update(&mut self.fps_camera, self.settings.mouse_sensitivity as f32, elapsed);
            }
            CameraInUse::Orbit => {
                self.orbit_controller.rotate(dx, dy);
                offset.scale_assign_nonuniform(WORLD_FORWARDS);
                offset.scale_assign(SPEED);
                let z = offset.x() + offset.y() + offset.z();
                self.orbit_controller.r#move(z);
                self.orbit_controller.update(&mut self.orbit_camera, self.settings.mouse_sensitivity as f32, elapsed);
            }
        }
    }
    fn handle_window_event(
        &mut self,
        event: winit::event::WindowEvent,
        window_id: &winit::window::WindowId,
    ) -> Result<bool> {
        use winit::event::WindowEvent;
        
        match event {
            WindowEvent::RedrawRequested => {
                self.execute_commands();
            }
            _ => {}
        }
        
        let (context, window) = self
            .windows
            .get_mut(window_id)
            .ok_or(Error::WindowIdInvalid)?;

        let event = match event {
            WindowEvent::CloseRequested => {
                tracing::debug!("close requested!");
                return Ok(true);
            }
            WindowEvent::Resized(s) => {
                unsafe { self.renderer.device.device_wait_idle() }
                    .inspect_err(|e| tracing::error!("{e}"))
                    .unwrap();

                {
                    let (w, h) = (s.width as f32, s.height as f32);
                    let aspect_ratio = w / h;

                    self.fps_camera.update_aspect_ratio(aspect_ratio);
                    self.orbit_camera.update_aspect_ratio(aspect_ratio);
                }

                let new_context = self.renderer.create_render_context(window)?;
                *context = new_context;

                return Ok(false);
            }
            WindowEvent::RedrawRequested => {
                // grabs cursor
                let rotation_condition_input = self
                    .binding_map
                    .get(&Command::Rotate)
                    .and_then(|&idx| self.settings.bindings.get(idx))
                    .and_then(|binding| binding.requirement)
                    .and_then(|idx| self.settings.bindings.get(idx))
                    .filter(|binding| matches!(binding.event, Event::Toggle))
                    .map(|binding| binding.input);
                for input in self.input_manager.all_just_pressed() {
                    if self.toggled.contains(input) {
                        if let Some(rci) = rotation_condition_input {
                            if rci == *input {
                                match window.set_cursor_grab(winit::window::CursorGrabMode::None) {
                                    Err(e) => {
                                        tracing::error!("{}", e);
                                    }
                                    _ => {}
                                }
                                window.set_cursor_visible(true);
                            }
                        }
                        self.toggled.remove(input);
                    } else {
                        if let Some(rci) = rotation_condition_input {
                            if rci == *input {
                                window
                                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                    .or_else(|_| {
                                        window.set_cursor_grab(
                                            winit::window::CursorGrabMode::Confined,
                                        )
                                    })
                                    .inspect_err(|e| tracing::error!("{e}"))?;
                                window.set_cursor_visible(false);
                            }
                        }
                        self.toggled.insert(*input);
                    }
                }

                match self.camera_in_use {
                    CameraInUse::Fps => {
                        let camera_ubo = renderer::CameraUBO {
                            view: self.fps_camera.view_matrix().into_2d_arr(),
                            proj: self.fps_camera.projection_matrix().into_2d_arr(),
                        };
                        context.update_camera(camera_ubo)?;
                    }
                    CameraInUse::Orbit => {
                        let camera_ubo = renderer::CameraUBO {
                            view: self.orbit_camera.view_matrix().into_2d_arr(),
                            proj: self.orbit_camera.projection_matrix().into_2d_arr(),
                        };
                        context.update_camera(camera_ubo)?;
                    }
                }
                
                
                let pipeline = context.get_pipeline();

                let temp = context.index as u32 * context.per_frame_buffer_element_size;

                let record_draw_commands = |cmd: vk::CommandBuffer| unsafe {
                    pipeline.bind(cmd);
                    {
                        let sets = [self.renderer.descriptor_sets[0]];
                        self.renderer.device.cmd_bind_descriptor_sets(
                            cmd,
                            self.renderer.pipeline_layout.bind_point,
                            self.renderer.pipeline_layout.handle,
                            0,
                            &sets,
                            &[temp],
                        );
                    }
                    {
                        let sets = [self.renderer.descriptor_sets[2]];
                        self.renderer.device.cmd_bind_descriptor_sets(
                            cmd,
                            self.renderer.pipeline_layout.bind_point,
                            self.renderer.pipeline_layout.handle,
                            2,
                            &sets,
                            &[],
                        );
                    }

                    let itr = match &self.camera_in_use {
                        CameraInUse::Fps => self.draw_infos.iter().skip(0),
                        CameraInUse::Orbit => self.draw_infos.iter().skip(1)
                    };
                    for (vb, ib, mesh_idx) in itr {
                        {
                            let sets = [self.renderer.descriptor_sets[1]];
                            self.renderer.device.cmd_bind_descriptor_sets(
                                cmd,
                                self.renderer.pipeline_layout.bind_point,
                                self.renderer.pipeline_layout.handle,
                                1,
                                &sets,
                                &[*mesh_idx
                                    * self.renderer.model_transform_buffer_element_size as u32],
                            );
                        }
                        vb.bind(cmd);
                        ib.bind(cmd);
                        ib.draw(cmd);
                    }
                };

                unsafe { context.draw(record_draw_commands) }?;

                window.request_redraw();

                return Ok(false);
            }
            e => e,
        };

        self.input_manager.update(InputEvent::Window(event));

        Ok(false)
    }
}
impl ApplicationHandler for Application {
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if self.exiting {
            return;
        }

        self.exiting = true;

        return event_loop.exit();
    }
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.input_manager.start_frame();
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }

        let window_attributes =
            winit::window::WindowAttributes::default().with_title(self.window_name.clone());
        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("{}", e);
                return self.exiting(event_loop);
            }
        };

        let window_id = window.id();

        let context = match self.renderer.create_render_context(&window) {
            Ok(context) => context,
            Err(e) => {
                tracing::error!("{}", e);
                return self.exiting(event_loop);
            }
        };
        {
            let s = window.inner_size();
            let (w, h) = (s.width as f32, s.height as f32);
            let aspect_ratio = w / h;

            self.fps_camera.update_aspect_ratio(aspect_ratio);
        };

        self.renderer
            .update_world_light(
                self.global_ambient_light,
                self.global_light_direction,
                self.global_light_color,
            )
            .unwrap();

        self.windows.insert(window_id, (context, window));
    }

    #[allow(unused_variables)]
    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.input_manager.update(InputEvent::Device(event));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        if self.exiting {
            return;
        }

        match self.handle_window_event(event, &window_id) {
            Ok(b) => {
                if b {
                    self.exiting(event_loop);
                }
            }
            Err(e) => {
                tracing::error!("{}", e);
                self.exiting(event_loop);
            }
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_file(true)
        .with_line_number(true)
        .init();

    let args: Box<[String]> = std::env::args().collect();
    let name = format!(
        "{}",
        std::env::current_exe()?.file_name().unwrap().display()
    );

    let print_usage = || -> Result<()> {
        println!(
            "Invalid program arguments. Usage: {} <options> <model>",
            name.clone()
        );
        println!("To view all options type {} --help", name);
        return Ok(());
    };

    if args.len() < 2 {
        return print_usage();
    }

    if let Some(_) = args.iter().find(|s| s.as_str() == "--help") {
        println!("Options:");
        println!("    --settings. Defaults to files/default_settings.yaml.");
        return Ok(());
    }

    let model_path = {
        let args: Vec<String> = std::env::args().collect();
        std::path::PathBuf::from(args[args.len() - 1].clone())
    };

    let settings =
        {
            let arg_idx = args.iter().enumerate().find_map(|(idx, arg)| {
                if arg == "--settings" { Some(idx) } else { None }
            });

            let path_str = if let Some(idx) = arg_idx {
                args.get(idx + 1)
            } else {
                Some(&String::from_str("files/default_settings.yaml").unwrap())
            };

            if let Some(str) = path_str {
                let path = match std::path::PathBuf::from_str(str) {
                    Ok(p) => p,
                    Err(e) => {
                        println!("Could not load settings. Error info: {e}");
                        return Ok(());
                    }
                };

                Settings::new(&path, &args)?
            } else {
                println!("Settings file not present");
                return Ok(());
            }
        };

    let event_loop = EventLoop::new().inspect_err(|e| tracing::error!("{e}"))?;

    let mut app = {
        let debug_enabled = cfg!(debug_assertions);
        let owned_display_handle = event_loop.owned_display_handle();
        let display_handle = owned_display_handle.display_handle()?;
        Application::new(
            name.into_boxed_str(),
            settings,
            model_path.as_path(),
            debug_enabled,
            &display_handle,
        )?
    };

    event_loop
        .run_app(&mut app)
        .inspect_err(|e| tracing::error!("{e}"))?;

    Ok(())
}
