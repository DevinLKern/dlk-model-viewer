mod camera;
mod constants;
mod input_manager;
mod result;
mod settings;

use camera::{Camera, controllers::*};
use constants::*;
use input_manager::{Input, InputEvent, InputManager};
use obj_mtl::{Vertex, VertexNormal};
use renderer::{
    CameraUBO, FrameContext, GridRenderPass, MainRenderPass, Scene, SceneBuilder, ShaderVertVertex,
};
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

use math::{Identity, Mat4, Quat, Vec3, Vec4, Zero};

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
    arrow_camera: Camera,
    windows: HashMap<WindowId, (renderer::FrameContext, Window)>,
    renderer: renderer::Renderer,
    main_scene: Scene,
    main_pass: MainRenderPass,
    grid_pass: GridRenderPass,
    default_texture_index: usize,
    grid_first_vertex: usize,
    grid_index_count: usize,
    grid_first_index: usize,
    // (first_index_count, index_count, material_index)
    model_shape_info: Vec<(usize, usize, usize)>,
    model_import_transform: math::Mat4<f32>,
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
    fn calc_derived_normal(v0: &Vertex, v1: &Vertex, v2: &Vertex) -> VertexNormal {
        let v0 = Vec3::new(v0.x as f32, v0.y as f32, v0.z as f32);
        let v1 = Vec3::new(v1.x as f32, v1.y as f32, v1.z as f32);
        let v2 = Vec3::new(v2.x as f32, v2.y as f32, v2.z as f32);
        let n = v1.sub(v0).cross(v2.sub(v0)).normalized();
        VertexNormal {
            x: n.x() as f64,
            y: n.y() as f64,
            z: n.z() as f64,
        }
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
        let obj_scene = obj_mtl::ObjScene::from_file(model_path)?;

        let mtl_materials = match obj_mtl::load_materials(&file_path) {
            Ok(materials) => materials,
            Err(obj_mtl::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("INFO: Could not find {}", file_path.display());
                Box::new([])
            }
            Err(e) => return Err(e.into()),
        };

        // TODO: one is added to account for the plane, default texture, and default material.
        // However, this is very unsafe. add bounds checks and return errors instead of crashing
        // or printing validation error info.
        let mut renderer = renderer::Renderer::new(debug_enabled, display_handle)?;

        let mut scene_builder = SceneBuilder::new();

        scene_builder.set_light_color(Vec4::new(1.0, 1.0, 1.0, 1.0));
        scene_builder.set_light_direction(ENGINE_RIGHT.sub(ENGINE_UP));
        scene_builder.set_ambient_light_intensity(0.1);

        // main - add materials and textures
        let mut texture_path_to_index = HashMap::<Box<str>, usize>::new();
        let mut material_name_to_index = HashMap::<Box<str>, usize>::new();

        let default_texture_index = {
            let image =
                image::load_from_memory_with_format(DEFAULT_IMAGE, image::ImageFormat::Png)?;
            let image = renderer.create_image(image)?;
            scene_builder.add_image(renderer.repeat_sampler(), image)
        };

        let default_material_index =
            scene_builder.add_material(Vec4::new(1.0, 0.2, 0.2, 1.0), None, false);

        for material in mtl_materials.iter() {
            if let Some(_material_index) = material_name_to_index.get(&material.name) {
                continue;
            }

            let base_color = material.diffuse.color.unwrap_or([0.0; 3]);
            let base_color = Vec4::new(base_color[0], base_color[1], base_color[2], 1.0);

            let diffuse_texture = if let Some(texture) = &material.diffuse.texture {
                let image_index = if let Some(index) = texture_path_to_index.get(&texture.file_path)
                {
                    *index
                } else {
                    let path = {
                        let base = model_path.with_file_name("");
                        // PathBuf::from_str is infallible
                        let target = PathBuf::from_str(&texture.file_path).unwrap();

                        Self::search_for(&base, &target).ok_or(Error::CouldNotFindFile)?
                    };
                    let image = image::open(&path).inspect_err(|e| tracing::error!("{e}"))?;
                    let image = renderer.create_image(image)?;
                    let image_index = scene_builder.add_image(renderer.repeat_sampler(), image);

                    texture_path_to_index.insert(texture.file_path.clone(), image_index);
                    image_index
                };

                Some(image_index)
            } else {
                None
            };

            let material_index = scene_builder.add_material(base_color, diffuse_texture, false);
            material_name_to_index.insert(material.name.clone(), material_index);
        }

        // main - add model vertices
        let shape_vertex_offset = 0;
        let mut vertex_map = HashMap::<obj_mtl::VtnIndex, usize>::new();
        let mut model_min = Vec3::scalar(f32::MAX);
        let mut model_max = Vec3::scalar(f32::MIN);
        // Vec<(index_count, first_index, material_info)>
        let mut model_shape_info = Vec::<(usize, usize, usize)>::new();
        for shape in obj_scene.get_shapes() {
            let triangles = shape.get_primitives().flat_map(|p| match p {
                obj_mtl::Primitive::Triangle { v0, v1, v2 } => vec![(*v0, *v1, *v2)].into_iter(),
                obj_mtl::Primitive::Polygon(indices) => (2..indices.len())
                    .map(move |i| (indices[0], indices[i - 1], indices[i]))
                    .collect::<Box<[_]>>()
                    .into_iter(),
                _ => Vec::new().into_iter(),
            });

            let vertices = triangles
                .flat_map(|(v0, v1, v2)| {
                    let dn = if settings.derive_normals {
                        Self::calc_derived_normal(
                            &obj_scene.vs[v0.v],
                            &obj_scene.vs[v1.v],
                            &obj_scene.vs[v2.v],
                        )
                    } else {
                        VertexNormal {
                            x: 0.0,
                            y: 0.0,
                            z: 0.0,
                        }
                    };
                    [(v0, dn), (v1, dn), (v2, dn)].into_iter()
                })
                .filter_map(|(v, dn)| {
                    let idx = vertex_map.len() + shape_vertex_offset;

                    match vertex_map.entry(v) {
                        std::collections::hash_map::Entry::Occupied(_) => None,
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            let position = obj_scene.vs[v.v];

                            let tex_coord =
                                v.vt.and_then(|idx| obj_scene.vts.get(idx))
                                    .copied()
                                    .unwrap_or_default();

                            let normal = v.vn.and_then(|idx| obj_scene.vns.get(idx)).unwrap_or(&dn);

                            let position =
                                Vec3::new(position.x as f32, position.y as f32, position.z as f32);

                            model_max = model_max.max(position);
                            model_min = model_min.min(position);

                            entry.insert(idx);

                            Some(ShaderVertVertex {
                                position: position.into_arr(),
                                tex_coord: [tex_coord.u as f32, 1.0 - tex_coord.v as f32],
                                normal: [normal.x as f32, normal.y as f32, normal.z as f32],
                            })
                        }
                    }
                });

            let (_first_vertex, _vertex_count) = scene_builder.add_vertices(vertices);

            let triangles = shape.get_primitives().flat_map(|p| match p {
                obj_mtl::Primitive::Triangle { v0, v1, v2 } => vec![(*v0, *v1, *v2)].into_iter(),
                obj_mtl::Primitive::Polygon(indices) => (2..indices.len())
                    .map(move |i| (indices[0], indices[i - 1], indices[i]))
                    .collect::<Vec<_>>()
                    .into_iter(),
                _ => Vec::new().into_iter(),
            });

            let indices = triangles
                .flat_map(|(v0, v1, v2)| [v0, v1, v2].into_iter())
                .map(|v| *vertex_map.get(&v).unwrap() as u32);

            let (first_index, index_count) = scene_builder.add_indices(indices);

            if shape.materials.len() > 1 {
                println!("Warning: Multiple materials per shape not supported.");
            }

            let material_index = shape
                .materials
                .get(0)
                .and_then(|mtl_name| material_name_to_index.get(mtl_name))
                .unwrap_or(&default_material_index);

            model_shape_info.push((first_index, index_count, *material_index));
        }

        let model_scale = model_max.sub(model_min);
        let model_scale = model_scale.x().max(model_scale.y()).max(model_scale.z());
        let model_scale = 1.0 / model_scale;

        let model_import_transform = {
            let center = model_max.add(model_min).scaled(0.5);
            let t = Mat4::translation(Vec3::ZERO.sub(center));
            let r = settings.from_model.into_mat4(1.0);

            r.mul(&t)
        };

        let model_transform = math::AffineTransform {
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            scalar: Vec3::scalar(model_scale),
        };

        let mut binding_map = HashMap::new();
        for (index, binding) in settings.bindings.iter().enumerate() {
            binding_map.insert(binding.command, index);
        }

        let (orbit_camera, orbit_controller) = {
            let mut controller = OrbitCameraController::new(model_transform.position);
            let mut camera = Camera::orthographic(1.25, 1.25, 100.0);

            camera
                .transform
                .translate_global(model_transform.position.sub(ENGINE_FORWARDS));
            controller.update(&mut camera, 0.0, 0.0);

            (camera, controller)
        };

        let (fps_camera, fps_controller) = {
            let mut controller = FpsCameraController::new();
            let mut camera = Camera::perspective(settings.fov_y);

            controller.r#move(model_transform.position.sub(ENGINE_FORWARDS));
            controller.update(&mut camera, 1.0, 1.0);

            (camera, controller)
        };

        let global_light_direction = Vec3::ZERO.sub(ENGINE_UP).add(ENGINE_RIGHT.scaled(0.2));

        let camera_in_use = settings.default_camera.clone();

        // grid - add vertices
        const PS: f32 = 1000.0;
        const PLANE_VERTEX_BUFFER_DATA: &[renderer::ShaderVertVertex] = {
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
                    position: FL.scaled(PS).into_arr(),
                    tex_coord: [0.0; 2],
                    normal: [0.0; 3],
                },
                renderer::ShaderVertVertex {
                    position: FR.scaled(PS).into_arr(),
                    tex_coord: [0.0; 2],
                    normal: [0.0; 3],
                },
                renderer::ShaderVertVertex {
                    position: BR.scaled(PS).into_arr(),
                    tex_coord: [0.0; 2],
                    normal: [0.0; 3],
                },
                renderer::ShaderVertVertex {
                    position: BL.scaled(PS).into_arr(),
                    tex_coord: [0.0; 2],
                    normal: [0.0; 3],
                },
            ]
        };
        const PLANE_INDEX_BUFFER_DATA: &[u32] = &[0, 1, 2, 2, 3, 0];

        let (grid_first_vertex, _grid_vertex_count) =
            scene_builder.add_vertices(PLANE_VERTEX_BUFFER_DATA.iter().map(|v| ShaderVertVertex {
                position: v.position,
                tex_coord: v.tex_coord,
                normal: v.normal,
            }));
        let (grid_first_index, grid_index_count) = scene_builder.add_indices(
            PLANE_INDEX_BUFFER_DATA
                .iter()
                .map(|i| *i + grid_first_vertex as u32),
        );

        let main_scene =
            scene_builder.build(renderer.device.clone(), renderer.mesh_arenas_mut())?;

        let main_pass =
            renderer::MainRenderPass::new(renderer.device.clone(), &main_scene, &mut renderer)?;

        let grid_pass =
            renderer::GridRenderPass::new(renderer.device.clone(), &main_scene, &mut renderer)?;

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
            arrow_camera: Camera::orthographic(1.5, 1.5, 10.0),
            windows: HashMap::new(),
            main_scene,
            main_pass,
            grid_pass,
            grid_first_vertex,
            grid_index_count,
            grid_first_index,
            default_texture_index,
            model_shape_info,
            model_import_transform,
            model_transform,
            global_light_direction,
            global_light_color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            global_ambient_light: 0.05,
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

        {
            self.arrow_camera.transform.position = Vec3::ZERO;
            let current_camera = match self.camera_in_use {
                CameraInUse::Fps => &self.fps_camera,
                CameraInUse::Orbit => &self.orbit_camera,
            };
            self.arrow_camera.transform.orientation =
                current_camera.transform.orientation.inverse();
            self.arrow_camera
                .transform
                .translate_local(Vec3::ZERO.sub(ENGINE_FORWARDS));
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

                let new_context = {
                    let ctx = renderer::FrameContext::new(self.renderer.device.clone(), &window)?;
                    self.main_pass.update_context(&ctx);
                    self.grid_pass.update_context(&ctx);
                    ctx
                };
                *context = new_context;

                return Ok(false);
            }
            WindowEvent::RedrawRequested => {
                self.execute_commands(window_id)?;

                let (context, window) = self
                    .windows
                    .get_mut(window_id)
                    .ok_or(Error::WindowIdInvalid)?;

                let camera_data = {
                    let cur_camera = match self.camera_in_use {
                        CameraInUse::Fps => &self.fps_camera,
                        CameraInUse::Orbit => &self.orbit_camera,
                    };

                    CameraUBO {
                        view_matrix: cur_camera.view_matrix().as_2d_arr(),
                        proj_matrix: cur_camera.projection_matrix().as_2d_arr(),
                    }
                };

                let swapchain_extent = context.swapchain_extent();

                let record_draw_commands = |ctx: &mut FrameContext| -> renderer::Result<()> {
                    ctx.get_current_frame_mut().allocator_mut().reset();
                    let cmd = ctx.get_current_frame().command_buffer();

                    // PART 1 - MODEL
                    unsafe {
                        let scissor = vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: swapchain_extent,
                        };
                        let viewport = ash::vk::Viewport {
                            x: 0.0,
                            y: 0.0,
                            width: scissor.extent.width as f32,
                            height: scissor.extent.height as f32,
                            min_depth: 0.0,
                            max_depth: 1.0,
                        };
                        self.renderer.device.cmd_set_viewport(cmd, 0, &[viewport]);
                        self.renderer.device.cmd_set_scissor(cmd, 0, &[scissor]);

                        self.main_scene.reset();

                        for (first_index, index_count, material_index) in
                            self.model_shape_info.iter()
                        {
                            let submesh_index =
                                self.main_scene.add_submesh(*first_index, *index_count);
                            let transform = self
                                .model_transform
                                .as_mat4()
                                .mul(&self.model_import_transform);

                            let instance_index =
                                self.main_scene.add_instance(transform, *material_index);
                            let _draw = self.main_scene.add_draw(instance_index, submesh_index);
                        }

                        self.renderer.render_main_scene(
                            ctx,
                            &self.main_scene,
                            &self.main_pass,
                            camera_data,
                        )?;
                    }

                    // PART 2 - GRID
                    unsafe {
                        let scissor = vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: swapchain_extent,
                        };
                        let viewport = ash::vk::Viewport {
                            x: 0.0,
                            y: 0.0,
                            width: scissor.extent.width as f32,
                            height: scissor.extent.height as f32,
                            min_depth: 0.0,
                            max_depth: 1.0,
                        };
                        self.renderer.device.cmd_set_viewport(cmd, 0, &[viewport]);
                        self.renderer.device.cmd_set_scissor(cmd, 0, &[scissor]);

                        self.main_scene.reset();

                        let submesh_index = self
                            .main_scene
                            .add_submesh(self.grid_first_index, self.grid_index_count);
                        self.main_scene.add_draw(0, submesh_index);

                        self.renderer.render_grid_scene(
                            ctx,
                            &self.main_scene,
                            &self.grid_pass,
                            camera_data,
                        )?;
                    }

                    Ok(())
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

        let window_attributes = winit::window::WindowAttributes::default()
            .with_title(self.window_name.clone())
            .with_min_inner_size(winit::dpi::Size::Physical(winit::dpi::PhysicalSize {
                width: 256,
                height: 256,
            }));
        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("{}", e);
                return self.exiting(event_loop);
            }
        };

        let window_id = window.id();

        let context = match renderer::FrameContext::new(self.renderer.device.clone(), &window) {
            Ok(ctx) => {
                self.main_pass.update_context(&ctx);
                self.grid_pass.update_context(&ctx);
                ctx
            }
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

        if let Some((_old_context, _)) = self.windows.insert(window_id, (context, window)) {
            //
        }
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
