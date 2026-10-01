use crate::{
    camera::{self, controllers::CameraController, *},
    constants::*,
    input_manager::{Input, InputEvent, InputManager},
    settings, {Command, Error, Event, Result, Settings},
};
use obj_mtl::{Vertex, VertexNormal};
use renderer::{
    DepthResources, DepthTechnique, FrameContextImageHandle, FrameContextRange, GridInstanceUBO,
    GridMaterialData, GridMaterialUBO, GridResources, GridResourcesRegistry, GridTechnique,
    GridVertVertex, InstanceUBO, MAX_INDIRECT_COMMAND_DATA_COUNT, MainMaterialUBO, MainResources,
    MainTechnique, MaterialHandle, MeshData, MeshNodeRegistry, RenderStorage, RenderTarget,
    RenderTechnique, Renderer, ShaderVertVertex, TypedSubMeshHandle,
};

use ash::vk;

use std::str::FromStr;
use std::u8;
use std::{
    collections::HashMap,
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use winit::{
    application::ApplicationHandler,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use math::{Identity, Mat4, Quat, Vec2, Vec3, Vec4, Zero};

pub const MAX_POINT_LIGHT_COUNT: u64 = 32;
const DEFAULT_IMAGE: &[u8] = include_bytes!("../../files/images/default.png");

#[derive(Default)]
struct FrameContextState {
    pub camera_data_range: renderer::FrameContextRange<renderer::Uniform>,
    pub instance_data_range: renderer::FrameContextRange<renderer::Storage>,
    pub grid_instance_data_range: renderer::FrameContextRange<renderer::Storage>,
    pub point_light_count_ubo_data_range: renderer::FrameContextRange<renderer::Storage>,
    pub point_light_data_range: renderer::FrameContextRange<renderer::Storage>,
    pub global_light_data_range: renderer::FrameContextRange<renderer::Storage>,
    pub directional_light_data_range: renderer::FrameContextRange<renderer::Uniform>,
    pub indirect_command_range: vulkan::AllocationRange,
    pub grid_indirect_command_range: vulkan::AllocationRange,
    pub depth_image_handle: FrameContextImageHandle,
}

impl FrameContextState {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl renderer::HasBindingValue<renderer::InstanceBuffer> for FrameContextState {
    type Value = FrameContextRange<renderer::Storage>;
    fn get(&self) -> Self::Value {
        self.instance_data_range
    }
}
impl renderer::HasBindingValue<renderer::CameraUBO> for FrameContextState {
    type Value = FrameContextRange<renderer::Uniform>;
    fn get(&self) -> Self::Value {
        self.camera_data_range
    }
}
impl renderer::HasBindingValue<renderer::DepthImage> for FrameContextState {
    type Value = renderer::FrameContextImageHandle;
    fn get(&self) -> Self::Value {
        self.depth_image_handle
    }
}
impl renderer::HasBindingValue<renderer::PointLightsUBO> for FrameContextState {
    type Value = FrameContextRange<renderer::Storage>;
    fn get(&self) -> Self::Value {
        let start = self
            .point_light_count_ubo_data_range
            .offset
            .min(self.point_light_data_range.offset);
        let end1 = self.point_light_count_ubo_data_range.offset
            + self.point_light_count_ubo_data_range.size;
        let end2 = self.point_light_data_range.offset + self.point_light_data_range.size;
        let end = end1.max(end2);
        let size = end - start;
        vulkan::AllocationRange {
            offset: start,
            size,
        }
        .into()
    }
}
impl renderer::HasBindingValue<renderer::DirectionalLightUBO> for FrameContextState {
    type Value = FrameContextRange<renderer::Uniform>;
    fn get(&self) -> Self::Value {
        self.directional_light_data_range
    }
}
impl renderer::HasBindingValue<renderer::GridInstanceBuffer> for FrameContextState {
    type Value = FrameContextRange<renderer::Storage>;
    fn get(&self) -> Self::Value {
        self.grid_instance_data_range
    }
}
#[derive(Debug, Copy, Clone)]
pub enum CameraInUse {
    Fps,
    Orbit,
}

#[allow(unused)]
pub struct Application {
    last: std::time::Instant,
    window_name: Box<str>,
    settings: Settings,
    binding_map: HashMap<Command, usize>,
    input_manager: InputManager,
    toggled: HashSet<Input>,
    camera_in_use: CameraInUse,
    fps_camera: Camera,
    fps_controller: camera::controllers::FpsCameraController,
    orbit_camera: Camera,
    orbit_controller: camera::controllers::OrbitCameraController,
    windows: HashMap<WindowId, (renderer::FrameContext, Window)>,
    renderer: renderer::Renderer,
    frame_state: FrameContextState,
    main_resources: MainResources,
    main_technique: MainTechnique,
    grid_resources: GridResources,
    grid_technique: GridTechnique,
    depth_resources: DepthResources,
    depth_technique: DepthTechnique,
    default_texture_handle: renderer::MainImageHandle,
    model_import_transform: math::Mat4<f32>,
    model_transform: math::AffineTransform,
    mesh_data: MeshData<ShaderVertVertex, u32>,
    draws: Box<
        [(
            MaterialHandle<MainMaterialUBO>,
            TypedSubMeshHandle<ShaderVertVertex, u32>,
        )],
    >,
    grid_mesh_data: MeshData<GridVertVertex, u32>,
    grid_draws: Box<
        [(
            MaterialHandle<GridMaterialUBO>,
            TypedSubMeshHandle<GridVertVertex, u32>,
        )],
    >,
    global_light_direction: Vec3<f32>,
    global_light_color: Vec4<f32>,
    global_ambient_light: f32,
    exiting: bool,
}

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
    pub fn new(
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

        let mut scene_resources = renderer::ResourceRegistry::new(&renderer);

        let mut main_resources = renderer::MainResourcesRegistry::new();

        let global_light_direction = ENGINE_RIGHT
            .scaled(0.5)
            .add(ENGINE_FORWARDS.scaled(0.3))
            .sub(ENGINE_UP);

        main_resources.set_global_light_ubo_data(renderer::GlobalLightUBO {
            direction: global_light_direction
                .normalized()
                .into_vec4(1.0)
                .into_arr(),
            color: [1.0; 4],
            ambient: 0.1,
        });

        // load materials and textures
        let mut texture_path_to_handle = HashMap::<Arc<str>, renderer::MainImageHandle>::new();
        let mut material_name_to_handle =
            HashMap::<Arc<str>, renderer::MaterialHandle<MainMaterialUBO>>::new();

        let default_texture_handle = {
            let image =
                image::load_from_memory_with_format(DEFAULT_IMAGE, image::ImageFormat::Png)?;
            let image = renderer.create_and_populate_image(image, vk::SampleCountFlags::TYPE_1)?;
            let image_handle = scene_resources.add_image(image);
            main_resources.add_image(image_handle, renderer.repeat_sampler())
        };
        let default_material_handle = main_resources.add_material(
            renderer::MainMaterialData {
                diffuse_base: Vec3::new(1.0, 0.2, 0.2),
                diffuse_texture: None,
                ambient_base: Vec3::scalar(0.0),
                ambient_texture: None,
                specular_base: Vec3::scalar(0.25),
                specular_texture: None,
                shininess: 24.0,
            },
            &mut scene_resources,
        )?;

        for material in mtl_materials.iter() {
            let name: Arc<str> = material.name.clone().into();
            if let Some(_material_index) = material_name_to_handle.get(&name) {
                continue;
            }

            fn get_textured_value<T: Copy>(
                tv: &obj_mtl::TexturedValue<T>,
                fallback_value: T,
                renderer: &mut Renderer,
                main_resources: &mut renderer::MainResourcesRegistry,
                resources: &mut renderer::ResourceRegistry,
                texture_path_to_handle: &mut HashMap<Arc<str>, renderer::MainImageHandle>,
                model_path: &Path,
            ) -> Result<(T, Option<renderer::MainImageHandle>)> {
                let value = tv.value.unwrap_or(fallback_value);
                let handle = if let Some(texture) = &tv.texture {
                    let path = {
                        let base = model_path.with_file_name("");
                        // PathBuf::from_str is infallible
                        let target = PathBuf::from_str(&texture.file_path).unwrap();

                        Application::search_for(&base, &target).ok_or(Error::CouldNotFindFile)?
                    };

                    let image = image::open(&path).inspect_err(|e| tracing::error!("{e}"))?;
                    let image =
                        renderer.create_and_populate_image(image, vk::SampleCountFlags::TYPE_1)?;
                    let image_handle = resources.add_image(image);
                    let image_handle =
                        main_resources.add_image(image_handle, renderer.repeat_sampler());

                    texture_path_to_handle.insert(texture.file_path.clone().into(), image_handle);

                    Some(image_handle)
                } else {
                    None
                };

                Ok((value, handle))
            }

            let (diffuse_base, diffuse_texture) = get_textured_value(
                &material.diffuse,
                [1.0; 3],
                &mut renderer,
                &mut main_resources,
                &mut scene_resources,
                &mut texture_path_to_handle,
                model_path,
            )?;
            let (ambient_base, ambient_texture) = get_textured_value(
                &material.ambient,
                [0.0; 3],
                &mut renderer,
                &mut main_resources,
                &mut scene_resources,
                &mut texture_path_to_handle,
                model_path,
            )?;
            let (specular_base, specular_texture) = get_textured_value(
                &material.specular,
                [0.0; 3],
                &mut renderer,
                &mut main_resources,
                &mut scene_resources,
                &mut texture_path_to_handle,
                model_path,
            )?;
            let material_handle = main_resources.add_material(
                renderer::MainMaterialData {
                    diffuse_base: Vec3::new(diffuse_base[0], diffuse_base[1], diffuse_base[2]),
                    diffuse_texture,
                    ambient_base: Vec3::new(ambient_base[0], ambient_base[1], ambient_base[2]),
                    ambient_texture,
                    specular_base: Vec3::new(specular_base[0], specular_base[1], specular_base[2]),
                    specular_texture,
                    shininess: 24.0,
                },
                &mut scene_resources,
            )?;
            material_name_to_handle.insert(material.name.clone().into(), material_handle);
        }

        let mut scene_vertices = renderer::MeshNodeRegistry::new();

        // load model vertices
        let mut vertex_map = HashMap::<obj_mtl::VtnIndex, u32>::new();
        let mut model_min = Vec3::scalar(f32::MAX);
        let mut model_max = Vec3::scalar(f32::MIN);
        let mut draws = Vec::new();

        while let Some(shape) = obj_scene.next_shape() {
            let mut vertices = Vec::with_capacity(32);
            let mut indices = Vec::new();

            for (v0, v1, v2) in shape.primitives().flat_map(|primitive| match primitive {
                obj_mtl::Primitive::Triangle { v0, v1, v2 } => vec![(*v0, *v1, *v2)].into_iter(),
                obj_mtl::Primitive::Polygon(indices) => (2..indices.len())
                    .map(move |i| (indices[0], indices[i - 1], indices[i]))
                    .collect::<Box<[_]>>()
                    .into_iter(),
                _ => Vec::new().into_iter(),
            }) {
                let derived_normal = if settings.derive_normals {
                    Self::calc_derived_normal(
                        &obj_scene.get_vertex(v0.v).copied().unwrap(),
                        &obj_scene.get_vertex(v1.v).copied().unwrap(),
                        &obj_scene.get_vertex(v2.v).copied().unwrap(),
                    )
                } else {
                    VertexNormal {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    }
                };

                for v in [v0, v1, v2] {
                    let index = if let Some(&index) = vertex_map.get(&v) {
                        index
                    } else {
                        let position = obj_scene.get_vertex(v.v).copied().unwrap();
                        let position =
                            Vec3::new(position.x as f32, position.y as f32, position.z as f32);
                        let tex_coord =
                            v.vt.and_then(|idx| obj_scene.get_vertex_texture(idx))
                                .copied();
                        let normal =
                            v.vn.and_then(|idx| obj_scene.get_vertex_normal(idx).copied())
                                .unwrap_or(derived_normal);

                        model_max = model_max.max(position);
                        model_min = model_min.min(position);

                        let index = vertex_map
                            .len()
                            .try_into()
                            .expect("vertex count exceeds u32");
                        vertices.push(ShaderVertVertex {
                            position: position.into_arr(),
                            tex_coord: tex_coord
                                .map(|uv| [uv.u as f32, 1.0 - uv.v as f32])
                                .unwrap_or([0.0, 0.0]),
                            normal: [normal.x as f32, normal.y as f32, normal.z as f32],
                        });

                        vertex_map.insert(v, index);
                        index
                    };

                    indices.push(index);
                }
            }

            if shape.material_ranges.len() > 1 {
                println!("Warning: Multiple materials per shape not supported.");
            }

            let material_handle = shape
                .material_ranges
                .get(0)
                .and_then(|(name, _idx, _count)| material_name_to_handle.get(name))
                .unwrap_or(&default_material_handle);

            let submesh_handle =
                scene_vertices.add_submesh(indices.into_iter(), vertices.into_iter());

            draws.push((*material_handle, submesh_handle));
        }

        // grid
        let mut grid_mesh_data = MeshNodeRegistry::new();
        let mut grid_resources = GridResourcesRegistry::new();
        let mut grid_draws = Vec::new();
        {
            const PS: f32 = 1000.0;
            const PLANE_VERTEX_BUFFER_DATA: [GridVertVertex; 4] = {
                const F: Vec3<f32> = ENGINE_FORWARDS;
                const B: Vec3<f32> = Vec3::ZERO.sub(ENGINE_FORWARDS);
                const R: Vec3<f32> = ENGINE_RIGHT;
                const L: Vec3<f32> = Vec3::ZERO.sub(ENGINE_RIGHT);

                const FR: Vec3<f32> = F.add(R);
                const FL: Vec3<f32> = F.add(L);
                const BR: Vec3<f32> = B.add(R);
                const BL: Vec3<f32> = B.add(L);

                [
                    GridVertVertex {
                        position: FL.scaled(PS).into_arr(),
                    },
                    GridVertVertex {
                        position: FR.scaled(PS).into_arr(),
                    },
                    GridVertVertex {
                        position: BR.scaled(PS).into_arr(),
                    },
                    GridVertVertex {
                        position: BL.scaled(PS).into_arr(),
                    },
                ]
            };
            const PLANE_INDEX_BUFFER_DATA: [u32; 6] = [0, 1, 2, 2, 3, 0];

            let submesh = grid_mesh_data.add_submesh(
                PLANE_INDEX_BUFFER_DATA.into_iter(),
                PLANE_VERTEX_BUFFER_DATA.into_iter(),
            );

            let material = grid_resources.add_material(GridMaterialData {
                base_color: Vec4::scalar(0.0),
                line_color: Vec4::scalar(1.0),
                line_width: Vec2::new(0.01, 0.01),
                scale: Vec2::new(1.0, 1.0),
            });

            grid_draws.push((material, submesh));
        }
        let scene_vertices = scene_vertices.register(&mut scene_resources);
        let main_resources = main_resources.register(&mut scene_resources);
        let grid_mesh_data = grid_mesh_data.register(&mut scene_resources);
        let grid_resources = grid_resources.register(&mut scene_resources);

        let mut scene_resources = scene_resources.register(&renderer)?;

        let scene_vertices = scene_vertices.upload(&mut scene_resources)?;
        let main_resources = main_resources.upload(&mut scene_resources)?;
        let grid_mesh_data = grid_mesh_data.upload(&mut scene_resources)?;
        let grid_resources = grid_resources.upload(&mut scene_resources)?;

        let scene_resources = Arc::new(scene_resources.upload());

        let scene_vertices = scene_vertices.finalize(scene_resources.clone());
        let main_resources = main_resources.finalize(scene_resources.clone(), &mut renderer)?;
        let grid_mesh_data = grid_mesh_data.finalize(scene_resources.clone());
        let grid_resources = grid_resources.finalize(scene_resources, &mut renderer)?;

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
            let mut controller =
                camera::controllers::OrbitCameraController::new(model_transform.position);
            let mut camera = Camera::orthographic(1.25, 1.25, 10.0);

            camera
                .transform
                .translate_global(model_transform.position.sub(ENGINE_FORWARDS));
            controller.update(&mut camera, 1.0, 1.0);

            (camera, controller)
        };

        let (fps_camera, fps_controller) = {
            let mut controller = camera::controllers::FpsCameraController::new();
            let mut camera = Camera::perspective(settings.fov_y);

            controller.r#move(model_transform.position.sub(ENGINE_FORWARDS));
            controller.update(&mut camera, 1.0, 1.0);

            (camera, controller)
        };

        let camera_in_use = settings.default_camera.clone();

        let main_technique = renderer::MainTechnique::new(&mut renderer, &main_resources)?;
        let grid_technique = renderer::GridTechnique::new(&mut renderer, &grid_resources)?;
        let mut depth_resources = renderer::DepthResources::new(&mut renderer)?;
        let depth_technique = renderer::DepthTechnique::new(&mut renderer, &mut depth_resources)?;

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
            frame_state: FrameContextState::default(),
            main_resources,
            main_technique,
            depth_resources,
            grid_resources,
            grid_technique,
            depth_technique,
            default_texture_handle,
            model_import_transform,
            model_transform,
            mesh_data: scene_vertices,
            draws: draws.into_boxed_slice(),
            grid_draws: grid_draws.into_boxed_slice(),
            grid_mesh_data,
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
    fn reset_allocations(&mut self, ctx: &mut renderer::FrameContext) {
        ctx.reset_frames(&mut self.renderer);
        self.frame_state.reset();
    }
    fn update_context(&mut self, ctx: &mut renderer::FrameContext) -> Result<()> {
        let uniform_offset = self
            .renderer
            .device
            .get_uniform_buffer_min_offset_alignment();
        const CAMERA_SIZE: u64 = std::mem::size_of::<renderer::CameraUBO>() as u64;
        const INSTANCE_SIZE: u64 = std::mem::size_of::<InstanceUBO>() as u64;
        const GRID_INSTANCE_SIZE: u64 = std::mem::size_of::<GridInstanceUBO>() as u64;
        const POINT_LIGHT_COUNT_SIZE: u64 = std::mem::size_of::<renderer::PointLightsUBO>() as u64;
        const POINT_LIGHT_SIZE: u64 = std::mem::size_of::<renderer::PointLightUBO>() as u64;
        const GLOBAL_LIGHT_SIZE: u64 = std::mem::size_of::<renderer::GlobalLightUBO>() as u64;
        const DIRECTIONAL_LIGHT_SIZE: u64 =
            std::mem::size_of::<renderer::DirectionalLightUBO>() as u64;

        self.frame_state.camera_data_range = ctx
            .reserve_uniform_data(CAMERA_SIZE, CAMERA_SIZE)
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?
            .into();
        self.frame_state.instance_data_range = ctx
            .reserve_storage_data(
                renderer::MAX_INSTANCE_DATA_COUNT * INSTANCE_SIZE,
                INSTANCE_SIZE,
            )
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?
            .into();
        self.frame_state.grid_instance_data_range = ctx
            .reserve_storage_data(
                renderer::MAX_INSTANCE_DATA_COUNT * GRID_INSTANCE_SIZE,
                GRID_INSTANCE_SIZE,
            )
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?
            .into();

        let temp_alloc = ctx
            .reserve_storage_data(
                POINT_LIGHT_COUNT_SIZE + (MAX_POINT_LIGHT_COUNT * POINT_LIGHT_SIZE),
                POINT_LIGHT_COUNT_SIZE,
            )
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?;

        self.frame_state.point_light_count_ubo_data_range =
            FrameContextRange::new(temp_alloc.offset, POINT_LIGHT_COUNT_SIZE);
        self.frame_state.point_light_data_range = FrameContextRange::new(
            temp_alloc.offset + POINT_LIGHT_COUNT_SIZE,
            temp_alloc.size - POINT_LIGHT_COUNT_SIZE,
        );

        self.frame_state.global_light_data_range = ctx
            .reserve_storage_data(GLOBAL_LIGHT_SIZE, uniform_offset)
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?;
        self.frame_state.directional_light_data_range = ctx
            .reserve_uniform_data(DIRECTIONAL_LIGHT_SIZE, uniform_offset)
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?
            .into();
        const SIZE_INDIRECT: u64 = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;
        const DRAW_COUNT: u64 = MAX_INDIRECT_COMMAND_DATA_COUNT / 3;
        self.frame_state.indirect_command_range = ctx
            .reserve_indirect_data(DRAW_COUNT * SIZE_INDIRECT, SIZE_INDIRECT)
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?;

        self.frame_state.grid_indirect_command_range = ctx
            .reserve_indirect_data(DRAW_COUNT * SIZE_INDIRECT, SIZE_INDIRECT)
            .ok_or_else(|| {
                self.reset_allocations(ctx);
                renderer::Error::BufferCapacityExceeded
            })?;

        self.frame_state.depth_image_handle = {
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
        }
        .inspect_err(|_| {
            self.reset_allocations(ctx);
        })?;

        self.main_technique
            .update_context(ctx, &self.frame_state, &self.renderer)
            .inspect_err(|_| {
                self.reset_allocations(ctx);
            })?;
        self.grid_technique
            .update_context(ctx, &self.frame_state, &self.renderer)
            .inspect_err(|_| {
                self.reset_allocations(ctx);
            })?;
        self.depth_technique
            .update_context(ctx, &self.frame_state, &self.renderer)
            .inspect_err(|_| {
                self.reset_allocations(ctx);
            })?;

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
            // TODO: I'm not sure I need this anymore
            old_context.destroy_images();
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

                // TODO: not sure I need this
                context.destroy_images();
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
                    renderer::CameraUBO {
                        view_matrix: cur_camera.view_matrix().as_2d_arr(),
                        proj_matrix: cur_camera.projection_matrix().as_2d_arr(),
                    }
                };

                let mut indirect_command_data =
                    Vec::<vk::DrawIndexedIndirectCommand>::with_capacity(64);
                let mut instance_data = Vec::<InstanceUBO>::with_capacity(64);

                let indirect_command_stride =
                    std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;
                let instance_stride = {
                    let size = std::mem::size_of::<InstanceUBO>();
                    let align = std::mem::align_of::<InstanceUBO>();

                    size.next_multiple_of(align) as u64
                };

                let first_instance_offset =
                    (self.frame_state.instance_data_range.offset / instance_stride) as u32;

                let model_matrix = self
                    .model_transform
                    .as_mat4()
                    .mul(&self.model_import_transform);
                for (material_handle, sumbmesh_handle) in self.draws.iter() {
                    let submesh = self.mesh_data.get_submesh(*sumbmesh_handle).unwrap();
                    let normal_matrix = model_matrix
                        .as_mat3()
                        .transposed()
                        .inverse()
                        .unwrap_or(math::Mat3::IDENTITY)
                        .into_mat4(1.0);
                    let instance_ubo = InstanceUBO {
                        model_matrix: model_matrix.as_2d_arr(),
                        material_index: material_handle.index(),
                        normal_matrix: normal_matrix.as_2d_arr(),
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                    };
                    let draw_command = vk::DrawIndexedIndirectCommand {
                        first_index: submesh.first_index(),
                        index_count: submesh.index_count(),
                        instance_count: 1,
                        vertex_offset: 0,
                        first_instance: instance_data.len() as u32,
                    };
                    indirect_command_data.push(draw_command);
                    instance_data.push(instance_ubo);
                }

                unsafe {
                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .storage_allocator_mut()
                        .upload_data(self.frame_state.instance_data_range.into(), &instance_data)
                        .map_err(|e| renderer::Error::VulkanError(e))?;

                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .indirect_allocator_mut()
                        .upload_data(
                            self.frame_state.indirect_command_range.into(),
                            &indirect_command_data,
                        )
                        .map_err(|e| renderer::Error::VulkanError(e))?;

                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .uniform_allocator_mut()
                        .upload_data(self.frame_state.camera_data_range.into(), &[camera_data])
                        .map_err(|e| renderer::Error::VulkanError(e))?;
                }

                let swapchain_extent = context.swapchain_extent();

                let cmd = context.get_current_frame().command_buffer();

                context
                    .get_current_frame_mut()
                    .allocator_mut()
                    .indirect_allocator_mut()
                    .reset();

                let swapchain_target = context.get_swapchain_render_target()?;

                let (depth_pass, depth_target) = {
                    let depth_image = context
                        .get_image(self.frame_state.depth_image_handle)
                        .unwrap();
                    let (width, height) = (depth_image.width, depth_image.height);
                    let pass = renderer::RenderPass {
                        color: Box::new([]),
                        depth: Some(renderer::Attachment {
                            load_op: vk::AttachmentLoadOp::CLEAR,
                            store_op: vk::AttachmentStoreOp::STORE,
                            clear_val: vk::ClearValue {
                                depth_stencil: vk::ClearDepthStencilValue {
                                    depth: 1.0,
                                    stencil: 0,
                                },
                            },
                            final_layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                        }),
                    };

                    let target = renderer::FrameContextRenderTarget {
                        color_images: Box::new([]),
                        depth_image: Some(self.frame_state.depth_image_handle),
                        render_area: vk::Rect2D {
                            offset: vk::Offset2D { x: 0, y: 0 },
                            extent: vk::Extent2D { width, height },
                        },
                    };

                    (pass, target)
                };

                depth_pass.begin_rendering(&depth_target, context, &mut self.renderer, cmd)?;

                {
                    unsafe {
                        let (scissor, viewport) = depth_target.get_default_scissor_and_viewport();
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
                            .r#move(camera.transform.position.sub(self.global_light_direction));
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
                            .uniform_allocator_mut()
                            .upload_data(
                                self.frame_state.directional_light_data_range.into(),
                                &light_data,
                            )
                            .map_err(|e| renderer::Error::VulkanError(e))?;
                    }
                }

                self.mesh_data.bind(cmd, &self.renderer);

                self.depth_resources
                    .bind(cmd, &mut self.renderer, &depth_target, context);

                self.renderer.render(
                    context,
                    &self.depth_technique,
                    &self.depth_resources,
                    self.frame_state.indirect_command_range.offset,
                    indirect_command_data.len() as u32,
                    indirect_command_stride as u32,
                )?;

                depth_pass.end_rendering(&depth_target, context, &mut self.renderer, cmd)?;

                let main_pass = renderer::RenderPass {
                    color: Box::new([renderer::Attachment {
                        load_op: vk::AttachmentLoadOp::CLEAR,
                        store_op: vk::AttachmentStoreOp::STORE,
                        clear_val: vk::ClearValue {
                            color: vk::ClearColorValue { float32: [0.0; 4] },
                        },
                        final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                    }]),
                    depth: Some(renderer::Attachment {
                        load_op: vk::AttachmentLoadOp::CLEAR,
                        store_op: vk::AttachmentStoreOp::STORE,
                        clear_val: vk::ClearValue {
                            depth_stencil: vk::ClearDepthStencilValue {
                                depth: 1.0,
                                stencil: 0,
                            },
                        },
                        final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                    }),
                };

                main_pass.begin_rendering(&swapchain_target, context, &mut self.renderer, cmd)?;

                {
                    unsafe {
                        let (scissor, viewport) =
                            swapchain_target.get_default_scissor_and_viewport();
                        self.renderer.device.cmd_set_viewport(cmd, 0, &[viewport]);
                        self.renderer.device.cmd_set_scissor(cmd, 0, &[scissor]);
                    }
                    let camera = match self.camera_in_use {
                        CameraInUse::Fps => &self.fps_camera,
                        CameraInUse::Orbit => &self.orbit_camera,
                    };
                    let point_light_data = [renderer::PointLightUBO {
                        color: [1.0, 1.0, 1.0, 0.1],
                        position: camera.transform.position.as_arr(),
                        _pad: 0,
                    }];
                    unsafe {
                        context
                            .get_current_frame_mut()
                            .allocator_mut()
                            .storage_allocator_mut()
                            .upload_data(
                                self.frame_state.point_light_data_range.into(),
                                &point_light_data,
                            )
                            .map_err(|e| renderer::Error::VulkanError(e))
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
                            .storage_allocator_mut()
                            .upload_data(
                                self.frame_state.point_light_count_ubo_data_range.into(),
                                &point_light_count_data,
                            )
                            .map_err(|e| renderer::Error::VulkanError(e))
                    }?;
                }

                self.main_resources
                    .bind(cmd, &mut self.renderer, &swapchain_target, context)?;

                self.mesh_data.bind(cmd, &self.renderer);

                self.renderer.render(
                    context,
                    &self.main_technique,
                    &self.main_resources,
                    self.frame_state.indirect_command_range.offset,
                    indirect_command_data.len() as u32,
                    indirect_command_stride as u32,
                )?;

                // grid
                indirect_command_data.clear();
                let mut grid_instance_data = Vec::with_capacity(8);
                for (material_handle, submesh_handle) in self.grid_draws.iter() {
                    let submesh = self.grid_mesh_data.get_submesh(*submesh_handle).unwrap();
                    let instance_ubo = GridInstanceUBO {
                        model_matrix: Mat4::IDENTITY.as_2d_arr(),
                        _pad: [[0.0; 4]; 4],
                        material_index: material_handle.index(),
                        _pad0: 0,
                        _pad1: 0,
                        _pad2: 0,
                    };
                    let draw_command = vk::DrawIndexedIndirectCommand {
                        first_index: submesh.first_index(),
                        index_count: submesh.index_count(),
                        instance_count: 1,
                        vertex_offset: 0,
                        first_instance: grid_instance_data.len() as u32,
                    };
                    indirect_command_data.push(draw_command);
                    grid_instance_data.push(instance_ubo);
                }
                unsafe {
                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .storage_allocator_mut()
                        .upload_data(
                            self.frame_state.grid_instance_data_range.into(),
                            &grid_instance_data,
                        )
                        .map_err(|e| renderer::Error::VulkanError(e))?;

                    context
                        .get_current_frame_mut()
                        .allocator_mut()
                        .indirect_allocator_mut()
                        .upload_data(
                            self.frame_state.grid_indirect_command_range,
                            &indirect_command_data,
                        )
                        .map_err(|e| renderer::Error::VulkanError(e))?;
                }

                self.grid_technique
                    .bind(cmd, context, &self.grid_resources, &self.renderer)?;

                self.grid_resources
                    .bind(cmd, &mut self.renderer, &swapchain_target, context);

                self.grid_mesh_data.bind(cmd, &self.renderer);

                self.renderer.render(
                    context,
                    &self.grid_technique,
                    &self.grid_resources,
                    self.frame_state.grid_indirect_command_range.offset,
                    indirect_command_data.len() as u32,
                    indirect_command_stride as u32,
                )?;

                main_pass.end_rendering(&swapchain_target, context, &mut self.renderer, cmd)?;
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
