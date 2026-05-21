mod camera;
mod constants;
mod input_manager;
mod result;
mod settings;

use camera::{Camera, controllers::*};
use constants::*;
use input_manager::{Input, InputEvent, InputManager};
use renderer::ShaderVertVertex;
use result::{Error, Result};
use settings::{Command, Event, Settings};

use ash::vk;
use vulkan::{IndexBuffer, VertexBuffer};

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

use math::{AffineTransform, Identity, Quat, Vec3, Vec4, Zero};

include!(concat!(env!("OUT_DIR"), "/arrow.rs"));

#[derive(Debug, Copy, Clone)]
enum CameraInUse {
    Fps,
    Orbit,
}

#[allow(unused)]
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
    default_texture_index: usize,
    vertex_buffer: vulkan::VertexBuffer,
    index_buffer: vulkan::IndexBuffer,
    draw_command_count: u32,
    indirect_command_buffer: vulkan::Buffer,
    model_transform: math::AffineTransform,
    global_light_direction: Vec3<f32>,
    global_light_color: Vec4<f32>,
    global_ambient_light: f32,
    exiting: bool,
}

const DEFAULT_IMAGE: &[u8] = include_bytes!("../../files/images/default.png");
const DEFAULT_SETTINGS: &str = include_str!("../../files/default_settings.yaml");

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
        let objf = obj_mtl::ObjScene::from_file(model_path)?;

        let mtl_materials = match obj_mtl::load_materials(&file_path) {
            Ok(materials) => materials,
            Err(obj_mtl::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("INFO: Could not find {}", file_path.display());
                Box::new([])
            }
            Err(e) => return Err(e.into()),
        };

        let (texture_count, material_count) = {
            let mut count = 0;
            let mut seen = HashSet::<&str>::new();
            for material in mtl_materials.iter() {
                if let Some(texture) = &material.diffuse.texture {
                    if seen.insert(&texture.file_path) {
                        count += 1;
                    }
                }
            }

            (count, mtl_materials.len() as u64)
        };

        let shape_count: u64 = objf.get_shapes().map(|_| 1).sum();

        // TODO: one is added to account for the plane, default texture, and default material.
        // However, this is very unsafe. add bounds checks and return errors instead of crashing
        // or printing validation error info.
        let mut renderer = renderer::Renderer::new(
            debug_enabled,
            display_handle,
            shape_count + 4,
            texture_count + 1,
            material_count + 4,
        )?;

        let mut texture_name_to_index = HashMap::<Box<str>, usize>::new();
        let default_texture_index = {
            let image_data =
                image::load_from_memory_with_format(DEFAULT_IMAGE, image::ImageFormat::Png)?;
            renderer.create_image(image_data)?
        };
        for material in mtl_materials.iter() {
            if let Some(diffuse_texture) = &material.diffuse.texture {
                // PathBuf::from_str is infallable
                let path = {
                    let base = model_path.with_file_name("");
                    // PathBuf::from_str is infallible
                    let target = PathBuf::from_str(&diffuse_texture.file_path).unwrap();

                    Self::search_for(&base, &target).ok_or(Error::CouldNotFindFile)?
                };
                let name = diffuse_texture.file_path.clone();
                if texture_name_to_index.contains_key(&name) {
                    continue;
                }
                let texture_data = image::open(path).inspect_err(|e| tracing::error!("{e}"))?;
                let texture_handle = renderer.create_image(texture_data)?;
                texture_name_to_index.insert(name, texture_handle);
            }
        }

        let mut material_name_to_index = HashMap::<Box<str>, u64>::new();
        let default_material_index = renderer.add_material(renderer::MaterialUBO {
            flags: 0,
            texture_index: 0,
            _pad2: [0; 8],
            base_color: [0.8, 0.2, 0.2, 1.0],
        })?;
        for material in mtl_materials.iter() {
            let (texture_index, flags) = if let Some(diffuse_texture) = &material.diffuse.texture {
                let name = diffuse_texture.file_path.clone();
                (*texture_name_to_index.get(&name).unwrap() as u32, 1)
            } else {
                (0, 0)
            };

            let base_color = if let Some(color) = material.diffuse.color {
                [color[0], color[1], color[2], 1.0]
            } else {
                [0.0; 4]
            };

            let material_index = renderer.add_material(renderer::MaterialUBO {
                flags,
                texture_index,
                _pad2: [0; 8],
                base_color,
            })?;

            let name = material.name.clone();
            material_name_to_index.insert(name, material_index);
        }

        let mut vertices = Vec::<ShaderVertVertex>::with_capacity(1024);
        let mut indices = Vec::<u32>::with_capacity(512);
        let mut vertex_map = HashMap::<obj_mtl::VtnIndex, usize>::new();
        // transform, material_index
        let mut instance_info = Vec::<(AffineTransform, u64)>::with_capacity(32);
        let mut draw_commands = Vec::<vk::DrawIndexedIndirectCommand>::with_capacity(32);
        // let mut old_index_len = indices.len();
        // let mut old_vertex_len = vertices.len();
        
        const _PLANE_VERTEX_BUFFER_DATA: &[renderer::ShaderVertVertex] = {
            const F: Vec3<f32> = ENGINE_FORWARDS;
            const B: Vec3<f32> = Vec3::ZERO.sub(ENGINE_FORWARDS);
            const R: Vec3<f32> = ENGINE_RIGHT;
            const L: Vec3<f32> = Vec3::ZERO.sub(ENGINE_RIGHT);

            const FR: Vec3<f32> = F.add(R);
            const FL: Vec3<f32> = F.add(L);
            const BR: Vec3<f32> = B.add(R);
            const BL: Vec3<f32> = B.add(L);

            &[
                renderer::ShaderVertVertex {
                    position: FL.into_arr(),
                    tex_coord: [1.0, 0.0],
                    normal: ENGINE_UP.into_arr(),
                },
                renderer::ShaderVertVertex {
                    position: FR.into_arr(),
                    tex_coord: [0.0, 0.0],
                    normal: ENGINE_UP.into_arr(),
                },
                renderer::ShaderVertVertex {
                    position: BR.into_arr(),
                    tex_coord: [0.0, 1.0],
                    normal: ENGINE_UP.into_arr(),
                },
                renderer::ShaderVertVertex {
                    position: BL.into_arr(),
                    tex_coord: [1.0, 1.0],
                    normal: ENGINE_UP.into_arr(),
                },
            ]
        };
        const _PLANE_INDEX_BUFFER_DATA: &[u32] = &[0, 1, 2, 2, 3, 0];

        // old_vertex_len = vertices.len();
        // for vertex in PLANE_VERTEX_BUFFER_DATA {
        //     vertices.push(*vertex);
        // }
        // old_index_len = indices.len();
        // for index in PLANE_INDEX_BUFFER_DATA {
        //     indices.push(*index);
        // }
        // instance_info.push((
        //     math::AffineTransform {
        //         position: Vec3::ZERO.sub(ENGINE_UP).scaled(0.5),
        //         orientation: Quat::IDENTITY,
        //         scalar: Vec3::new(1.0, 1.0, 1.0),
        //     },
        //     0
        // ));

        let arrow_iter = crate::ARROW_VERTICES
            .iter()
            .zip(crate::ARROW_NORMALS.iter())
            .map(|(pos, normal)| renderer::ShaderVertVertex {
                position: *pos,
                tex_coord: [0.0, 0.0],
                normal: *normal,
            });
        for vertex in arrow_iter {
            vertices.push(vertex);
        }
        let mut old_index_len = indices.len();
        for index in crate::ARROW_INDICES {
            indices.push(*index);
        }
        let first_instance = instance_info.len() as u32;
        // red arrow (+X)
        let red_material_index = renderer.add_material(renderer::MaterialUBO {
            flags: 0,
            texture_index: 0,
            _pad2: [0; 8],
            base_color: [1.0, 0.1, 0.1, 1.0]
        })?;
        instance_info.push((
            AffineTransform {
                position: Vec3::ZERO,
                orientation: Quat::unit_from_angle_axis(std::f32::consts::FRAC_PI_2, Vec3::Z),
                scalar: ARROW_SCALAR,
            },
            red_material_index
        ));
        const ARROW_SCALAR: Vec3<f32> = Vec3::new(0.05, 0.1, 0.05);
        // green arrow (+Y)
        let green_material_index = renderer.add_material(renderer::MaterialUBO {
            flags: 0,
            texture_index: 0,
            _pad2: [0; 8],
            base_color: [0.1, 1.0, 0.1, 1.0]
        })?;
        instance_info.push((
            AffineTransform {
                position: Vec3::ZERO,
                orientation: Quat::unit_from_angle_axis(std::f32::consts::PI, Vec3::Z),
                scalar: ARROW_SCALAR,
            },
            green_material_index
        ));
        // blue arrow (+Z)
        let blue_material_index = renderer.add_material(renderer::MaterialUBO {
            flags: 0,
            texture_index: 0,
            _pad2: [0; 8],
            base_color: [0.1, 0.1, 1.0, 1.0]
        })?;
        instance_info.push((
            AffineTransform {
                position: Vec3::ZERO,
                orientation: Quat::unit_from_angle_axis(std::f32::consts::FRAC_PI_2, Vec3::X),
                scalar: ARROW_SCALAR,
            },
            blue_material_index
        ));
        draw_commands.push(vk::DrawIndexedIndirectCommand {
            index_count: (indices.len() - old_index_len) as u32,
            instance_count: 3,
            first_index: old_index_len as u32,
            vertex_offset: 0,
            first_instance,
        });
        
        let model_to_engine = crate::TO_ENGINE.mul(&settings.from_model);

        let old_instance_info_len = instance_info.len();
        let mut model_min = Vec3::scalar(f32::MAX);
        let mut model_max = Vec3::scalar(f32::MIN);
        for shape in objf.get_shapes() {
            let material_index = if shape.materials.len() == 0 {
                default_material_index
            } else if shape.materials.len() == 1 {
                *material_name_to_index
                    .get(&shape.materials[0])
                    .ok_or(Error::InvalidMaterialIndex)?
            } else {
                return Err(Error::MultipleMaterialsPerShape);
            };

            // Build a triangle list
            let mut triangles =
                Vec::<(obj_mtl::VtnIndex, obj_mtl::VtnIndex, obj_mtl::VtnIndex)>::with_capacity(64);
            for primitive in shape.get_primitives() {
                match primitive {
                    obj_mtl::Primitive::Triangle { v0, v1, v2 } => triangles.push((*v0, *v1, *v2)),
                    obj_mtl::Primitive::Polygon(poly) => {
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

            old_index_len = indices.len();

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

                            Some(face_normal)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                let derived_normal = match derived_normal {
                    Some(n) => n,
                    None => Vec3::new(0.0, 0.0, 0.0),
                };

                for v in [v0, v1, v2] {
                    let index = if let Some(&i) = vertex_map.get(&v) {
                        i
                    } else {
                        let position = model_to_engine
                            .mul_vec(Vec3::new(
                                objf.vs[v.v].x as f32,
                                objf.vs[v.v].y as f32,
                                objf.vs[v.v].z as f32,
                            ));

                        let tex_coord = if let Some(i) = v.vt {
                            [objf.vts[i].u as f32, 1.0 - objf.vts[i].v as f32]
                        } else {
                            [0.0, 0.0]
                        };

                        let normal = if let Some(i) = v.vn {
                            Vec3::new(
                                objf.vns[i].x as f32,
                                objf.vns[i].y as f32,
                                objf.vns[i].z as f32,
                            )
                        } else {
                            derived_normal
                        };

                        let normal = model_to_engine.mul_vec(normal).into_arr();

                        model_min = model_min.min(position);
                        model_max = model_max.max(position);

                        let i = vertices.len();
                        vertices.push(ShaderVertVertex {
                            position: position.into_arr(),
                            tex_coord,
                            normal,
                        });
                        vertex_map.insert(v, i);
                        i
                    };

                    indices.push(index as u32);
                }
            }

            draw_commands.push(vk::DrawIndexedIndirectCommand {
                index_count: (indices.len() - old_index_len) as u32,
                instance_count: 1,
                first_index: old_index_len as u32,
                vertex_offset: 0,
                first_instance: instance_info.len() as u32,
            });
            instance_info.push((
                AffineTransform {
                    position: Vec3::ZERO,
                    orientation: Quat::IDENTITY,
                    scalar: Vec3::scalar(1.0)
                },
                material_index
            ));
        }
        let model_position = Vec3::ZERO;
        let model_transform = {
            let center = model_max.add(model_min).scaled(0.5);
            let model_scale = model_max.sub(model_min);
            let model_scale = model_scale.x().max(model_scale.y()).max(model_scale.z());
            let model_scale = 1.0 / model_scale;

            let model_pos = model_position.sub(center.scaled(model_scale));

            math::AffineTransform {
                position: model_pos,
                orientation: Quat::IDENTITY,
                scalar: Vec3::scalar(model_scale),
            }
        };
        for (transform, _material_index) in instance_info.iter_mut().skip(old_instance_info_len) {
            *transform = model_transform;
        }

        let mut binding_map = HashMap::new();
        for (index, binding) in settings.bindings.iter().enumerate() {
            binding_map.insert(binding.command, index);
        }

        let (orbit_camera, orbit_controller) = {
            let mut controller = OrbitCameraController::new(model_position);
            let mut camera = Camera::orthographic(1.25, 1.25, 100.0);

            camera
                .transform
                .translate_global(model_position.add(ENGINE_FORWARDS));
            controller.update(&mut camera, 0.0, 0.0);

            (camera, controller)
        };

        let vertex_buffer = {
            let size = vertices.len() * std::mem::size_of::<ShaderVertVertex>();
            let data = unsafe {
                std::slice::from_raw_parts(
                    vertices.as_ptr() as *const u8,
                    size
                )
            };

            renderer.create_vertex_buffer(data, size as u32)?
        };

        let index_buffer = {
            let data = unsafe {
                std::slice::from_raw_parts(
                    indices.as_ptr() as *const u8,
                    indices.len() * std::mem::size_of::<u32>()
                )
            };
            renderer.create_index_buffer(
                data,
                vk::IndexType::UINT32,
                indices.len() as u32,
                0
            )?
        };

        for (transform, material_index) in instance_info {
            renderer.add_instance(transform.as_mat4(), material_index as u32)?;
        }

        let (fps_camera, fps_controller) = {
            let mut controller = FpsCameraController::new();
            let mut camera = Camera::perspective(settings.fov_y);

            controller.r#move(model_position.sub(ENGINE_FORWARDS));
            controller.update(&mut camera, 1.0, 1.0);

            (camera, controller)
        };

        let indirect_command_buffer = {
            let size = draw_commands.len() * std::mem::size_of::<vk::DrawIndexedIndirectCommand>();
            let buffer = renderer.create_indirect_buffer(
                size as u64
            )?;
 
            let dst = unsafe {
                let ptr = buffer.map_memory(0, size as u64).map_err(|e| renderer::Error::VulkanError(e.into()))?;
                std::slice::from_raw_parts_mut(ptr as *mut vk::DrawIndexedIndirectCommand, draw_commands.len())
            };

            dst.copy_from_slice(&draw_commands);

            unsafe { buffer.unmap(); }

            buffer
        };

        let global_light_direction = Vec3::ZERO.sub(ENGINE_UP).add(ENGINE_RIGHT.scaled(0.2));

        let camera_in_use = settings.default_camera.clone();

        Ok(Self {
            last: std::time::Instant::now(),
            window_name,
            settings,
            binding_map,
            input_manager: InputManager::new(),
            toggled: HashSet::<Input>::new(),
            renderer,
            camera_in_use,
            fps_camera,
            fps_controller,
            orbit_camera,
            orbit_controller,
            windows: HashMap::new(),
            default_texture_index,
            indirect_command_buffer,
            model_transform,
            vertex_buffer,
            index_buffer,
            draw_command_count: draw_commands.len() as u32,
            global_light_direction,
            global_light_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            global_ambient_light: 0.1,
            exiting: false,
        })
    }
    fn meets_requirements(&self, binding_index: usize) -> Option<bool> {
        let binding = self.settings.bindings.get(binding_index)?;
        let b = match binding.event {
            Event::Hold => self.input_manager.is_held(&binding.input),
            Event::Press => self.input_manager.just_pressed(&binding.input),
            Event::Release => self.input_manager.just_released(&binding.input),
            Event::Toggle => self.toggled.contains(&binding.input),
            Event::Movement => true,
        };

        let requirements_met = if let Some(idx) = binding.requirement {
            self.meets_requirements(idx)?
        } else {
            true
        };

        Some(b && requirements_met)
    }
    fn execute_commands(&mut self, window_id: &winit::window::WindowId) -> Result<()> {
        let (_, window) = self.windows.get(window_id).ok_or(Error::WindowIdInvalid)?;

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

        // hides and grabs or shows ad releases the cursor
        let rotation_condition_input = self
            .binding_map
            .get(&Command::Rotate)
            .and_then(|&idx| self.settings.bindings.get(idx))
            .and_then(|binding| binding.requirement)
            .and_then(|idx| self.settings.bindings.get(idx))
            .filter(|binding| matches!(binding.event, Event::Toggle))
            .map(|binding| binding.input);
        if let Some(input) = rotation_condition_input {
            use winit::window::CursorGrabMode;
            let toggled = self.toggled.contains(&input);
            if self.input_manager.just_released(&input) {
                if self.toggled.contains(&input) {
                    window.set_cursor_grab(CursorGrabMode::None)?;
                    self.toggled.remove(&input);
                } else {
                    window
                        .set_cursor_grab(CursorGrabMode::Locked)
                        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))?;
                    self.toggled.insert(input);
                }
            }
            window.set_cursor_visible(!toggled);
        }

        let mut offset = Vec3::ZERO;
        // camera should move at 2 units per second
        const SPEED: f32 = 2.0;
        const DIRS: &[(Command, Vec3<f32>)] = &[
            (Command::MoveForward, ENGINE_FORWARDS),
            (Command::MoveBackward, Vec3::ZERO.sub(ENGINE_FORWARDS)),
            (Command::MoveRight, ENGINE_RIGHT),
            (Command::MoveLeft, Vec3::ZERO.sub(ENGINE_RIGHT)),
            (Command::MoveUp, ENGINE_UP),
            (Command::MoveDown, Vec3::ZERO.sub(ENGINE_UP)),
        ];
        for (cmd, dir) in DIRS {
            let binding_index = match self.binding_map.get(cmd) {
                Some(x) => x,
                _ => continue,
            };

            if self.meets_requirements(*binding_index).unwrap() {
                offset.add_assign(*dir);
            }
        }
        offset = offset.normalized();
        match self.camera_in_use {
            CameraInUse::Fps => self.fps_controller.r#move(offset.scaled(SPEED)),
            CameraInUse::Orbit => {}
        }

        const DZ: f32 = 0.25;
        if let Some(idx) = self.binding_map.get(&Command::ZoomIn) {
            if let Some(true) = self.meets_requirements(*idx) {
                match &self.camera_in_use {
                    CameraInUse::Fps => self.fps_controller.zoom_delta += DZ,
                    CameraInUse::Orbit => self.orbit_controller.zoom_delta += DZ,
                }
            }
        }

        if let Some(idx) = self.binding_map.get(&Command::ZoomOut) {
            if let Some(true) = self.meets_requirements(*idx) {
                match &self.camera_in_use {
                    CameraInUse::Fps => self.fps_controller.zoom_delta -= DZ,
                    CameraInUse::Orbit => self.orbit_controller.zoom_delta -= DZ,
                }
            }
        }

        if let Some(idx) = self.binding_map.get(&Command::Rotate) {
            if let Some(true) = self.meets_requirements(*idx) {
                // NOTE: mouse_movement is the only valid input for rotate
                let (dx, dy) = self
                    .binding_map
                    .get(&Command::Rotate)
                    .and_then(|idx| self.meets_requirements(*idx))
                    .filter(|&ok| ok)
                    .map(|_| {
                        (
                            self.input_manager.mouse_delta.0,
                            self.input_manager.mouse_delta.1,
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                match self.camera_in_use {
                    CameraInUse::Fps => self.fps_controller.rotate(dx, dy),
                    CameraInUse::Orbit => self.orbit_controller.rotate(dx, dy),
                }
            }
        }

        let now = std::time::Instant::now();
        let elapsed = (now - self.last).as_secs_f64();
        self.last = now;
        match self.camera_in_use {
            CameraInUse::Fps => self.fps_controller.update(
                &mut self.fps_camera,
                self.settings.mouse_sensitivity,
                elapsed,
            ),
            CameraInUse::Orbit => self.orbit_controller.update(
                &mut self.orbit_camera,
                self.settings.mouse_sensitivity,
                elapsed,
            ),
        }

        Ok(())
    }
    fn handle_window_event(
        &mut self,
        event: winit::event::WindowEvent,
        window_id: &winit::window::WindowId,
    ) -> Result<bool> {
        use winit::event::WindowEvent;

        let event = match event {
            WindowEvent::CloseRequested => {
                tracing::debug!("close requested!");
                return Ok(true);
            }
            WindowEvent::Resized(s) => {
                let (context, window) = self
                    .windows
                    .get_mut(window_id)
                    .ok_or(Error::WindowIdInvalid)?;

                unsafe { self.renderer.device.device_wait_idle() }
                    .inspect_err(|e| tracing::error!("{e}"))
                    .unwrap();

                {
                    let (w, h) = (s.width as f32, s.height as f32);
                    let aspect_ratio = w / h;

                    self.fps_camera.set_aspect_ratio(aspect_ratio);
                    self.orbit_camera.set_aspect_ratio(aspect_ratio);
                }

                let new_context = self.renderer.create_render_context(window)?;
                *context = new_context;

                return Ok(false);
            }
            WindowEvent::RedrawRequested => {
                self.execute_commands(window_id)?;

                let (context, window) = self
                    .windows
                    .get_mut(window_id)
                    .ok_or(Error::WindowIdInvalid)?;

                match self.camera_in_use {
                    CameraInUse::Fps => {
                        let camera_ubo = renderer::CameraUBO {
                            view_matrix: self.fps_camera.view_matrix().into_2d_arr(),
                            proj_matrix: self.fps_camera.projection_matrix().into_2d_arr(),
                        };
                        context.update_camera(camera_ubo)?;
                    }
                    CameraInUse::Orbit => {
                        let camera_ubo = renderer::CameraUBO {
                            view_matrix: self.orbit_camera.view_matrix().into_2d_arr(),
                            proj_matrix: self.orbit_camera.projection_matrix().into_2d_arr(),
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
                        let sets = [self.renderer.descriptor_sets[1]];
                        self.renderer.device.cmd_bind_descriptor_sets(
                            cmd,
                            self.renderer.pipeline_layout.bind_point,
                            self.renderer.pipeline_layout.handle,
                            1,
                            &sets,
                            &[],
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

                    self.vertex_buffer.bind(cmd);
                    self.index_buffer.bind(cmd);

                    self.renderer.device.cmd_draw_indexed_indirect(
                        cmd,
                        self.indirect_command_buffer.handle,
                        0,
                        self.draw_command_count,
                        std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32
                    );
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

            self.fps_camera.set_aspect_ratio(aspect_ratio);
            self.orbit_camera.set_aspect_ratio(aspect_ratio);
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
    {
        const LEVEL: tracing::Level = if cfg!(debug_assertions) {
            tracing::Level::DEBUG
        } else {
            tracing::Level::ERROR
        };

        tracing_subscriber::fmt()
            .with_max_level(LEVEL)
            .with_file(true)
            .with_line_number(true)
            .init();
    }

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
        println!(
            "    --settings. This is an optional argument. Defaults to files/default_settings.yaml when unspecified."
        );
        return Ok(());
    }

    let model_path = {
        let args: Vec<String> = std::env::args().collect();
        std::path::PathBuf::from(args[args.len() - 1].clone())
    };

    let settings_dir = if let Some(dirs) = directories::ProjectDirs::from("", "", &name) {
        dirs.config_dir().to_path_buf()
    } else {
        println!("Could not find config directory!");
        return Ok(());
    };

    // ensure that default_settings.yaml exists
    {
        let settings_path = settings_dir.join("default_settings.yaml");

        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if !settings_path.exists() {
            std::fs::write(settings_path, DEFAULT_SETTINGS)?;
        }
    }

    let settings =
        {
            let arg_idx = args.iter().enumerate().find_map(|(idx, arg)| {
                if arg == "--settings" { Some(idx) } else { None }
            });

            let path_str = if let Some(idx) = arg_idx {
                args.get(idx + 1)
            } else {
                Some(&String::from_str("default_settings.yaml").unwrap())
            };

            if let Some(str) = path_str {
                let path = settings_dir.join(str);

                Settings::new(&path, &args)?
            } else {
                println!("Settings file not present!");
                return Ok(());
            }
        };

    let event_loop = EventLoop::new().inspect_err(|e| tracing::error!("{e}"))?;

    let name = model_path.display().to_string();

    let mut app = {
        const DEBUG_ENABLED: bool = cfg!(debug_assertions);
        let owned_display_handle = event_loop.owned_display_handle();
        let display_handle = owned_display_handle.display_handle()?;
        Application::new(
            name.into_boxed_str(),
            settings,
            model_path.as_path(),
            DEBUG_ENABLED,
            &display_handle,
        )?
    };

    event_loop
        .run_app(&mut app)
        .inspect_err(|e| tracing::error!("{e}"))?;

    Ok(())
}
