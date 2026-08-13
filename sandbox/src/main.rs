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
    AllocationRange, CameraUBO, DepthRenderPass, GridRenderPass, InstanceData, MainRenderPass,
    MaterialBuilderData, PointLightsUBO, Renderer, Scene, SceneBuilder, ShaderVertVertex,
    TextureIndexValue,
};
use result::{Error, Result};
use settings::{Command, Event, Settings};

use ash::vk;

use std::collections::HashSet;
use std::str::FromStr;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowId},
};

use math::{Identity, Mat3, Mat4, Quat, Vec3, Vec4, Zero};

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
    windows: HashMap<WindowId, (renderer::FrameContext, Window)>,
    renderer: renderer::Renderer,
    camera_data_range: AllocationRange,
    instance_data_range: AllocationRange,
    point_lights_ubo_data_range: AllocationRange,
    light_data_range: AllocationRange,
    main_scene: Scene,
    main_pass: MainRenderPass,
    grid_pass: GridRenderPass,
    depth_pass: DepthRenderPass,
    depth_image_index: usize,
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
        if let Ok(cwd) = std::env::current_dir() {
            let cwd_target = cwd.join(target);
            if cwd_target.exists() {
                return Some(cwd_target);
            }
        }

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
            x: n.x() as obj_mtl::Float,
            y: n.y() as obj_mtl::Float,
            z: n.z() as obj_mtl::Float,
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
        let mut obj_scene = obj_mtl::ShapeIterator::new(model_path)?;

        let mtl_materials = match obj_mtl::load_materials(&file_path) {
            Ok(materials) => materials,
            Err(obj_mtl::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("INFO: Could not find {}", file_path.display());
                Box::new([])
            }
            Err(e) => return Err(e.into()),
        };

        let mut renderer = {
            let target_samples = match settings.anti_aliasing {
                settings::AntiAliasing::MSAA64x => vk::SampleCountFlags::TYPE_64,
                settings::AntiAliasing::MSAA32x => vk::SampleCountFlags::TYPE_32,
                settings::AntiAliasing::MSAA16x => vk::SampleCountFlags::TYPE_16,
                settings::AntiAliasing::MSAA8x => vk::SampleCountFlags::TYPE_8,
                settings::AntiAliasing::MSAA4x => vk::SampleCountFlags::TYPE_4,
                settings::AntiAliasing::MSAA2x => vk::SampleCountFlags::TYPE_2,
                _ => vk::SampleCountFlags::TYPE_1,
            };
            let renderer = renderer::Renderer::new(debug_enabled, display_handle, target_samples)?;
            if renderer.samples() != target_samples {
                println!("INFO: Selected antialiasing setting not supported.");
            }
            renderer
        };

        let mut scene_builder = SceneBuilder::new();

        scene_builder.set_light_color(Vec4::new(1.0, 1.0, 1.0, 1.0));
        scene_builder.set_light_direction(
            ENGINE_RIGHT
                .scaled(0.5)
                .add(ENGINE_FORWARDS.scaled(0.3))
                .sub(ENGINE_UP)
                .normalized(),
        );
        scene_builder.set_ambient_light_intensity(0.1);

        // main - add materials and textures
        let mut texture_path_to_index = HashMap::<Arc<str>, usize>::new();
        let mut material_name_to_index = HashMap::<Arc<str>, usize>::new();

        let default_texture_index = {
            let image =
                image::load_from_memory_with_format(DEFAULT_IMAGE, image::ImageFormat::Png)?;
            let image = renderer.create_and_populate_image(image, vk::SampleCountFlags::TYPE_1)?;
            scene_builder.add_image(renderer.repeat_sampler(), image)
        };

        let default_material_index = scene_builder.add_material(MaterialBuilderData {
            diffuse: TextureIndexValue {
                value: [1.0, 0.2, 0.2],
                index: None,
            },
            ambient: TextureIndexValue {
                value: [0.0; 3],
                index: None,
            },
            specular: TextureIndexValue {
                value: [0.25; 3],
                index: None,
            },
            shininess: 24.0,
        });

        for material in mtl_materials.iter() {
            let name: Arc<str> = material.name.clone().into();
            if let Some(_material_index) = material_name_to_index.get(&name) {
                continue;
            }

            fn get_texture_index_value<T: Copy>(
                tv: &obj_mtl::TexturedValue<T>,
                fallback_value: T,
                renderer: &mut Renderer,
                scene_builder: &mut SceneBuilder,
                texture_path_to_index: &mut HashMap<Arc<str>, usize>,
                model_path: &Path,
            ) -> Result<TextureIndexValue<T>> {
                let value = tv.value.unwrap_or(fallback_value);
                let index = if let Some(texture) = &tv.texture {
                    let path = {
                        let base = model_path.with_file_name("");
                        // PathBuf::from_str is infallible
                        let target = PathBuf::from_str(&texture.file_path).unwrap();

                        Application::search_for(&base, &target).ok_or(Error::CouldNotFindFile)?
                    };

                    let image = image::open(&path).inspect_err(|e| tracing::error!("{e}"))?;
                    let image =
                        renderer.create_and_populate_image(image, vk::SampleCountFlags::TYPE_1)?;
                    let image_index = scene_builder.add_image(renderer.repeat_sampler(), image);

                    texture_path_to_index.insert(texture.file_path.clone().into(), image_index);

                    Some(image_index)
                } else {
                    None
                };

                Ok(TextureIndexValue { value, index })
            }

            let diffuse = get_texture_index_value(
                &material.diffuse,
                [1.0; 3],
                &mut renderer,
                &mut scene_builder,
                &mut texture_path_to_index,
                model_path,
            )?;
            let ambient = get_texture_index_value(
                &material.ambient,
                [0.0; 3],
                &mut renderer,
                &mut scene_builder,
                &mut texture_path_to_index,
                model_path,
            )?;
            let specular = get_texture_index_value(
                &material.specular,
                [0.0; 3],
                &mut renderer,
                &mut scene_builder,
                &mut texture_path_to_index,
                model_path,
            )?;
            let material_index = scene_builder.add_material(MaterialBuilderData {
                diffuse,
                ambient,
                specular,
                shininess: material.shininess.value.unwrap_or(0.0),
            });
            material_name_to_index.insert(material.name.clone().into(), material_index);
        }

        // main - add model vertices
        let shape_vertex_offset = 0;
        let mut vertex_map = HashMap::<obj_mtl::VtnIndex, usize>::new();
        let mut model_min = Vec3::scalar(f32::MAX);
        let mut model_max = Vec3::scalar(f32::MIN);
        // Vec<(index_count, first_index, material_info)>
        let mut model_shape_info = Vec::<(usize, usize, usize)>::new();
        while let Some(shape) = obj_scene.next_shape() {
            let triangles = shape.primitives().flat_map(|p| match p {
                obj_mtl::Primitive::Triangle { v0, v1, v2 } => vec![(*v0, *v1, *v2)].into_iter(),
                obj_mtl::Primitive::Polygon(indices) => (2..indices.len())
                    .map(move |i| (indices[0], indices[i - 1], indices[i]))
                    .collect::<Box<[_]>>()
                    .into_iter(),
                _ => Vec::new().into_iter(),
            });

            let first_index = scene_builder.indices_mut().len();

            for (v0, v1, v2) in triangles {
                let mut derived_normal = VertexNormal {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                };
                let indices = [v0, v1, v2].map(|v| {
                    let idx = vertex_map.len() + shape_vertex_offset;

                    match vertex_map.entry(v) {
                        std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            let position = obj_scene.get_vertex(v.v).copied().unwrap();

                            let tex_coord =
                                v.vt.and_then(|idx| obj_scene.get_vertex_texture(idx))
                                    .copied()
                                    .unwrap_or_default();

                            let normal =
                                v.vn.and_then(|idx| obj_scene.get_vertex_normal(idx).copied())
                                    .unwrap_or_else(|| {
                                        if settings.derive_normals {
                                            derived_normal = Self::calc_derived_normal(
                                                &obj_scene.get_vertex(v0.v).copied().unwrap(),
                                                &obj_scene.get_vertex(v1.v).copied().unwrap(),
                                                &obj_scene.get_vertex(v2.v).copied().unwrap(),
                                            )
                                        }
                                        derived_normal
                                    });

                            let position =
                                Vec3::new(position.x as f32, position.y as f32, position.z as f32);

                            model_max = model_max.max(position);
                            model_min = model_min.min(position);

                            scene_builder.vertices_mut().push(ShaderVertVertex {
                                position: position.into_arr(),
                                tex_coord: [tex_coord.u as f32, 1.0 - tex_coord.v as f32],
                                normal: [normal.x as f32, normal.y as f32, normal.z as f32],
                            });

                            *entry.insert(idx)
                        }
                    }
                });

                let _ = scene_builder.add_indices(indices.into_iter().map(|i| i as u32));
            }

            let index_count = scene_builder.indices_mut().len() - first_index;

            if shape.material_ranges.len() > 1 {
                println!("Warning: Multiple materials per shape not supported.");
            }

            let material_index = shape
                .material_ranges
                .get(0)
                .and_then(|(name, _idx, _count)| material_name_to_index.get(name))
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
            let mut camera = Camera::orthographic(1.25, 1.25, 10.0);

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

        let main_pass = renderer::MainRenderPass::new(&main_scene, &mut renderer)?;

        let grid_pass = renderer::GridRenderPass::new(&main_scene, &mut renderer)?;

        let depth_pass = renderer::DepthRenderPass::new(&mut renderer)?;

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
            camera_data_range: AllocationRange { offset: 0, size: 0 },
            instance_data_range: AllocationRange { offset: 0, size: 0 },
            point_lights_ubo_data_range: AllocationRange { offset: 0, size: 0 },
            light_data_range: AllocationRange { offset: 0, size: 0 },
            main_scene,
            main_pass,
            grid_pass,
            depth_pass,
            depth_image_index: 0,
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
    #[allow(unused)]
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
    #[allow(unused)]
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
    #[allow(unused)]
    fn update_context(&mut self, ctx: &mut renderer::FrameContext) -> Result<()> {
        const CAMERA_SIZE: u64 = std::mem::size_of::<CameraUBO>() as u64;
        const INSTANCE_SIZE: u64 = std::mem::size_of::<InstanceData>() as u64;
        const POINT_LIGHTS_UBO_SIZE: u64 = std::mem::size_of::<renderer::PointLightsUBO>() as u64;
        const POINT_LIGHT_SIZE: u64 = std::mem::size_of::<renderer::PointLightData>() as u64;
        const LIGHT_SIZE: u64 = std::mem::size_of::<renderer::DirectionalLightUBO>() as u64;

        self.camera_data_range = ctx
            .reserve_uniform_data(CAMERA_SIZE, CAMERA_SIZE)
            .ok_or_else(|| {
                ctx.destroy_images(&mut self.renderer);
                renderer::Error::BufferCapacityExceeded
            })?;
        self.instance_data_range = ctx
            .reserve_storage_data(
                renderer::MAX_INSTANCE_DATA_COUNT * INSTANCE_SIZE,
                INSTANCE_SIZE,
            )
            .ok_or_else(|| {
                ctx.destroy_images(&mut self.renderer);
                renderer::Error::BufferCapacityExceeded
            })?;
        self.point_lights_ubo_data_range = ctx
            .reserve_storage_data(
                POINT_LIGHTS_UBO_SIZE + (renderer::MAX_POINT_LIGHT_COUNT * POINT_LIGHT_SIZE),
                POINT_LIGHT_SIZE,
            )
            .ok_or_else(|| {
                ctx.destroy_images(&mut self.renderer);
                renderer::Error::BufferCapacityExceeded
            })?;

        self.light_data_range = ctx
            .reserve_uniform_data(LIGHT_SIZE, LIGHT_SIZE)
            .ok_or_else(|| {
                ctx.destroy_images(&mut self.renderer);
                renderer::Error::BufferCapacityExceeded
            })?;

        self.depth_image_index = {
            let image_create_info = vulkan::ImageCreateInfo {
                memory_property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                image_type: vk::ImageType::TYPE_2D,
                format: vk::Format::D32_SFLOAT,
                width: 1024 * 4,
                height: 1024 * 4,
                depth: 1,
                usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
                mip_level_count: 1,
                layer_count: 1,
                level_count: 1,
                samples: vk::SampleCountFlags::TYPE_1,
            };
            ctx.create_image(&image_create_info, &mut self.renderer)
        }?;

        self.main_pass.update_context(
            ctx,
            &self.camera_data_range,
            &self.instance_data_range,
            &self.point_lights_ubo_data_range,
            &self.light_data_range,
            self.depth_image_index,
            &self.renderer,
        )?;
        self.grid_pass.update_context(&ctx, &self.camera_data_range);
        self.depth_pass
            .update_context(&ctx, &self.light_data_range, &self.instance_data_range);

        Ok(())
    }
    #[allow(unused)]
    fn resumed_inner(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if !self.windows.is_empty() {
            return Ok(());
        }

        let window_attributes = winit::window::WindowAttributes::default()
            .with_title(self.window_name.clone())
            .with_min_inner_size(winit::dpi::Size::Physical(winit::dpi::PhysicalSize {
                width: 256,
                height: 256,
            }));
        let window = event_loop.create_window(window_attributes)?;

        let window_id = window.id();
        let context = {
            let mut ctx = unsafe { renderer::FrameContext::new(&mut self.renderer, &window) }?;

            self.update_context(&mut ctx)?;

            ctx
        };

        {
            let s = window.inner_size();
            let (w, h) = (s.width as f32, s.height as f32);
            let aspect_ratio = w / h;

            self.fps_camera.set_aspect_ratio(aspect_ratio);
            self.orbit_camera.set_aspect_ratio(aspect_ratio);
        }

        if let Some((mut old_context, _)) = self.windows.insert(window_id, (context, window)) {
            old_context.destroy_images(&mut self.renderer);
        }

        Ok(())
    }
    #[allow(unused)]
    fn window_event_inner(
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
                    let (_, window) = self
                        .windows
                        .get_mut(window_id)
                        .ok_or(Error::WindowIdInvalid)?;
                    let mut ctx =
                        unsafe { renderer::FrameContext::new(&mut self.renderer, &window) }?;

                    self.update_context(&mut ctx)?;

                    ctx
                };
                // fighting the borrow checker
                let (context, _) = self.windows.get_mut(window_id).unwrap();

                context.destroy_images(&mut self.renderer);
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

                let cmd = context.get_current_frame().command_buffer();

                self.main_scene.reset();
                context
                    .get_current_frame_mut()
                    .allocator_mut()
                    .reset_indirect();

                let target = context.get_swapchain_render_target()?;

                let depth_target = {
                    let depth_image = context
                        .get_current_frame()
                        .get_image(self.depth_image_index)
                        .unwrap();
                    let (width, height) = {
                        let img = self.renderer.get_image(depth_image).unwrap();
                        (img.width, img.height)
                    };
                    renderer::RenderTarget {
                        color: Box::new([]),
                        depth: Some(renderer::Attachment {
                            img: depth_image,
                            load_op: vk::AttachmentLoadOp::CLEAR,
                            store_op: vk::AttachmentStoreOp::STORE,
                            clear_val: vk::ClearValue {
                                depth_stencil: vk::ClearDepthStencilValue {
                                    depth: 1.0,
                                    stencil: 0,
                                },
                            },
                            final_layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                            resolve_img: None,
                        }),
                        render_area: vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: vk::Extent2D { width, height },
                        },
                    }
                };

                depth_target.begin_rendering(&mut self.renderer, cmd)?;

                let mut indirect_command_data =
                    Vec::<vk::DrawIndexedIndirectCommand>::with_capacity(64);
                let mut instance_data = Vec::<InstanceData>::with_capacity(64);

                let stride = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;
                for (first_index, index_count, material_index) in self.model_shape_info.iter() {
                    self.main_scene.add_submesh(*first_index, *index_count);

                    let model_matrix = self
                        .model_transform
                        .as_mat4()
                        .mul(&self.model_import_transform);

                    let normal_matrix = model_matrix
                        .as_mat3()
                        .transposed()
                        .inverse()
                        .unwrap_or(Mat3::IDENTITY)
                        .into_mat4(1.0);

                    indirect_command_data.push(vk::DrawIndexedIndirectCommand {
                        index_count: *index_count as u32,
                        instance_count: 1,
                        first_index: *first_index as u32,
                        vertex_offset: 0,
                        first_instance: instance_data.len() as u32,
                    });
                    instance_data.push(InstanceData {
                        model_matrix: model_matrix.as_2d_arr(),
                        normal_matrix: normal_matrix.as_2d_arr(),
                        material_index: *material_index as u32,
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                    });
                }

                let indirect_offset = unsafe {
                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .upload_storage_data(self.instance_data_range.offset, &instance_data)?;

                    let offset = context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .upload_indirect_data(&indirect_command_data, stride)?;

                    offset
                };
                unsafe {
                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .upload_uniform_data(self.camera_data_range.offset, &[camera_data])
                }?;

                // PART 0 - MESH

                {
                    unsafe {
                        let scissor = depth_target.render_area;
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
                    }
                    let light_data = {
                        let camera = match self.camera_in_use {
                            CameraInUse::Fps => &self.fps_camera,
                            CameraInUse::Orbit => &self.orbit_camera,
                        };

                        let mut light = camera::Camera::orthographic(3.0, 3.0, 4.0);
                        let mut controller = camera::controllers::FpsCameraController::new();
                        controller
                            .r#move(camera.transform.position.sub(self.main_scene.light_dir()));
                        controller.update(&mut light, 1.0, 1.0);
                        light.look_at(camera.transform.position, ENGINE_UP);

                        [renderer::DirectionalLightUBO {
                            view_matrix: light.view_matrix().into_2d_arr(),
                            proj_matrix: light.projection_matrix().into_2d_arr(),
                        }]
                    };

                    unsafe {
                        context
                            .get_current_frame_mut()
                            .allocator_mut()
                            .upload_uniform_data(self.light_data_range.offset, &light_data)?;
                    }
                }

                self.renderer.render_depth_scene(
                    context,
                    &self.main_scene,
                    &self.depth_pass,
                    indirect_offset,
                    indirect_command_data.len() as u32,
                    stride as u32,
                )?;

                // PART 1 - MODEL
                depth_target.end_rendering(&mut self.renderer, cmd)?;
                target.begin_rendering(&mut self.renderer, cmd)?;
                {
                    unsafe {
                        let scissor = target.render_area;
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
                    }
                    let camera = match self.camera_in_use {
                        CameraInUse::Fps => &self.fps_camera,
                        CameraInUse::Orbit => &self.orbit_camera,
                    };
                    let point_light_data = [renderer::PointLightData {
                        color: [1.0, 1.0, 1.0, 0.1],
                        position: camera.transform.position.as_arr(),
                        _pad: 0,
                    }];
                    unsafe {
                        context
                            .get_current_frame_mut()
                            .allocator_mut()
                            .upload_storage_data(
                                self.point_lights_ubo_data_range.offset
                                    + std::mem::size_of::<PointLightsUBO>() as u64,
                                &point_light_data,
                            )
                    }?;
                    let point_light_count_data = [renderer::PointLightsUBO {
                        count: point_light_data.len() as u32,
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                        arr: (),
                    }];
                    unsafe {
                        context
                            .get_current_frame_mut()
                            .allocator_mut()
                            .upload_storage_data(
                                self.point_lights_ubo_data_range.offset,
                                &point_light_count_data,
                            )
                    }?;
                }

                self.renderer.render_main_scene(
                    context,
                    &self.main_scene,
                    &self.main_pass,
                    indirect_offset,
                    indirect_command_data.len() as u32,
                    stride as u32,
                )?;

                // PART 2 - GRID
                self.main_scene.reset();
                instance_data.clear();
                indirect_command_data.clear();
                unsafe {
                    let scissor = vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: swapchain_extent,
                    };
                    let viewport = vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: scissor.extent.width as f32,
                        height: scissor.extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    };
                    self.renderer.device.cmd_set_viewport(cmd, 0, &[viewport]);
                    self.renderer.device.cmd_set_scissor(cmd, 0, &[scissor]);
                }

                let _submesh_index = self
                    .main_scene
                    .add_submesh(self.grid_first_index, self.grid_index_count);

                let (indirect_offset, draw_count, stride) = {
                    let frame = context.get_current_frame_mut();

                    let stride = {
                        let size = std::mem::size_of::<InstanceData>();
                        let align = std::mem::align_of::<InstanceData>();

                        size.next_multiple_of(align) as u64
                    };
                    let first_instance_offset = frame
                        .allocator()
                        .storage_buffer_offset()
                        .next_multiple_of(stride)
                        / stride;
                    indirect_command_data.push(vk::DrawIndexedIndirectCommand {
                        index_count: self.grid_index_count as u32,
                        instance_count: 1,
                        first_index: self.grid_first_index as u32,
                        vertex_offset: 0,
                        first_instance: 1 + first_instance_offset as u32,
                    });

                    let indirect_offset = unsafe {
                        frame.allocator_mut().upload_indirect_data(
                            &indirect_command_data,
                            std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64,
                        )
                    }?;

                    let draw_count = indirect_command_data.len() as u32;
                    let stride = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u32;

                    (indirect_offset, draw_count, stride)
                };

                self.renderer.render_grid_scene(
                    context,
                    &self.main_scene,
                    &self.grid_pass,
                    indirect_offset,
                    draw_count,
                    stride,
                )?;

                target.end_rendering(&mut self.renderer, cmd)?;
                context.submit()?;

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
        if let Err(e) = self.resumed_inner(event_loop) {
            tracing::error!("{}", e);
            self.exiting = true;
            event_loop.exit();
        }
    }
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
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

        match self.window_event_inner(event, &window_id) {
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
