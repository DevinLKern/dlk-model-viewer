use yaml_rust2::YamlLoader;

use crate::{CameraInUse, ENGINE_FORWARDS, ENGINE_RIGHT, ENGINE_UP, Input, Result, result::Error};

use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
};

#[allow(unused)]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Command {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    CameraRotation,
    ObjectRotation,
    Rotate,
    UseFpsCamera,
    UseOrbitCamera,
    ZoomIn,
    ZoomOut,
    Requirement,
}

impl Command {
    pub fn from_str(s: &str) -> Option<Self> {
        let command = s.to_lowercase();
        let command = match command.as_str() {
            "move_forward" => Command::MoveForward,
            "move_backward" => Command::MoveBackward,
            "move_left" => Command::MoveLeft,
            "move_right" => Command::MoveRight,
            "move_up" => Command::MoveUp,
            "move_down" => Command::MoveDown,
            "camera_rotation" => Command::CameraRotation,
            "object_rotation" => Command::ObjectRotation,
            "rotate" => Command::Rotate,
            "use_fps_camera" => Command::UseFpsCamera,
            "use_orbit_camera" => Command::UseOrbitCamera,
            "zoom_in" => Command::ZoomIn,
            "zoom_out" => Command::ZoomOut,
            // "rotate_forward_pos" => Command::RotateForwardPositive,
            // "rotate_forward_neg" => Command::RotateForwardNegative,
            // "rotate_right_pos" => Command::RotateRightPositive,
            // "rotate_right_neg" => Command::RotateRightNegative,
            // "rotate_up_pos" => Command::RotateUpPositive,
            // "rotate_up_neg" => Command::RotateUpNegative,
            _ => return None,
        };

        Some(command)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Event {
    Hold,
    Toggle,
    Press,
    Release,
    Movement,
}

impl Event {
    pub fn from_str(s: &str) -> Option<Self> {
        let s = s.to_lowercase();
        let res = match s.as_str() {
            "hold" => Self::Hold,
            "toggle" => Self::Toggle,
            "press" => Self::Press,
            "release" => Self::Release,
            "movement" => Self::Movement,
            _ => return None,
        };

        Some(res)
    }
}

#[derive(Debug)]
pub struct Binding {
    pub input: Input,
    pub command: Command,
    pub event: Event,
    pub requirement: Option<usize>,
}

#[derive(Debug)]
pub struct Settings {
    pub fov_y: f32,
    pub mouse_sensitivity: f64,
    pub derive_normals: bool,
    pub model_to_world: math::Mat3<f32>,
    pub bindings: Box<[Binding]>,
    pub _default_camera: CameraInUse,
}

impl Settings {
    pub fn new(settings_path: &std::path::Path, _args: &[String]) -> Result<Self> {
        use yaml_rust2::Yaml;

        let file = File::open(settings_path)?;
        let mut file_reader = BufReader::new(file);
        let mut contents = String::with_capacity(2048);
        let _ = file_reader.read_to_string(&mut contents)?;
        let objects = YamlLoader::load_from_str(&contents)?;
        let all = objects
            .first()
            .ok_or(Error::ConfigFileInvalid("no yaml objects"))?
            .as_hash()
            .ok_or(Error::ConfigFileInvalid("Bindings should be a yaml object"))?;

        let bindings_yaml = all
            .get(&Yaml::String(String::from("bindings")))
            .ok_or(Error::ConfigFileInvalid("bindings not found"))?
            .as_vec()
            .ok_or(Error::ConfigFileInvalid("expected bindings to be an array"))?;
        // let mut binding_command_names = HashMap::with_capacity(bindings_yaml.len());
        let mut binding_command_names = HashMap::with_capacity(bindings_yaml.len());
        for (i, binding_yaml) in bindings_yaml.iter().enumerate() {
            let b = if let Some(h) = binding_yaml.as_hash() {
                h
            } else {
                return Err(Error::ConfigFileInvalid("expected binding to be a has"));
            };
            let c = if let Some(c) = b.get(&Yaml::String(String::from("command"))) {
                c
            } else {
                return Err(Error::ConfigFileInvalid("command not found"));
            };
            let c = if let Some(c) = c.as_str() {
                c
            } else {
                return Err(Error::ConfigFileInvalid("expected command to be a string"));
            };

            if let Some(_) = binding_command_names.insert(c, i) {
                return Err(Error::ConfigFileInvalid("command name duplicated"));
            }
        }
        let mut bindings = Vec::with_capacity(bindings_yaml.len());
        for binding_yaml in bindings_yaml.iter().filter_map(|b| b.as_hash()) {
            let command = binding_yaml
                .get(&Yaml::String(String::from("command")))
                .unwrap()
                .as_str()
                .unwrap();
            let command = match Command::from_str(command) {
                Some(c) => c,
                _ => {
                    if !binding_command_names.contains_key(command) {
                        return Err(Error::ConfigFileInvalid("invalid command value"));
                    } else {
                        Command::Requirement
                    }
                }
            };

            let input = binding_yaml
                .get(&Yaml::String(String::from("input")))
                .ok_or(Error::ConfigFileInvalid("input not found"))?
                .as_str()
                .ok_or(Error::ConfigFileInvalid(
                    "expected input value to be a string",
                ))?;
            let input =
                Input::from_str(input).ok_or(Error::ConfigFileInvalid("invalid input value"))?;

            let event = binding_yaml
                .get(&Yaml::String(String::from("event")))
                .ok_or(Error::ConfigFileInvalid("event not found"))?
                .as_str()
                .ok_or(Error::ConfigFileInvalid(
                    "expected event value to be a string",
                ))?;
            let event =
                Event::from_str(event).ok_or(Error::ConfigFileInvalid("invlid event value"))?;

            let requirement = binding_yaml.get(&Yaml::String(String::from("requires")));
            let requirement = if let Some(r) = requirement {
                r.as_str()
            } else {
                None
            };
            let requirement = if let Some(r) = requirement {
                match binding_command_names.get(r) {
                    Some(idx) => Some(*idx),
                    _ => return Err(Error::ConfigFileInvalid("invalid requirement value")),
                }
            } else {
                None
            };

            bindings.push(Binding {
                command,
                input,
                event,
                requirement,
            });
        }

        let options = all
            .get(&Yaml::String(String::from("options")))
            .ok_or(Error::ConfigFileInvalid("options not found"))?
            .as_hash()
            .ok_or(Error::ConfigFileInvalid("expected options to be a hash"))?;

        let fov_y = options
            .get(&Yaml::String(String::from("fov_y")))
            .ok_or(Error::ConfigFileInvalid("fov_y not found"))?
            .as_f64()
            .ok_or(Error::ConfigFileInvalid("expected fov_y to be a float"))?;

        let mouse_sensitivity = options
            .get(&Yaml::String(String::from("mouse_sensitivity")))
            .ok_or(Error::ConfigFileInvalid("mouse_sensitivity not found"))?
            .as_f64()
            .ok_or(Error::ConfigFileInvalid(
                "expected mouse_sensitivity to be a float",
            ))?;

        let derive_normals = options
            .get(&Yaml::String(String::from("derive_normals")))
            .ok_or(Error::ConfigFileInvalid("derive_normals not found"))?
            .as_bool()
            .ok_or(Error::ConfigFileInvalid(
                "expected derive normals to be true",
            ))?;

        let str_to_vec = |s: &str| -> Option<math::Vec3<f32>> {
            let v = match s {
                "+x" => math::Vec3::new(1.0, 0.0, 0.0),
                "-x" => math::Vec3::new(-1.0, 0.0, 0.0),
                "+y" => math::Vec3::new(0.0, 1.0, 0.0),
                "-y" => math::Vec3::new(0.0, -1.0, 0.0),
                "+z" => math::Vec3::new(0.0, 0.0, 1.0),
                "-z" => math::Vec3::new(0.0, 0.0, -1.0),
                _ => return None,
            };

            Some(v)
        };

        let default_camera = options
            .get(&Yaml::String(String::from("default_camera")))
            .ok_or(Error::ConfigFileInvalid("default_camera not found"))?
            .as_str()
            .ok_or(Error::ConfigFileInvalid(
                "Expected default_camera to be a str",
            ))?;
        let default_camera = match default_camera {
            "fps" => CameraInUse::Fps,
            "orbit" => CameraInUse::Orbit,
            _ => return Err(Error::ConfigFileInvalid("invalid default_camera value")),
        };

        let model_up = options
            .get(&Yaml::String(String::from("model_up")))
            .ok_or(Error::ConfigFileInvalid("model_up value not found"))?
            .as_str()
            .ok_or(Error::ConfigFileInvalid("expected mode_up to be a str"))?;
        let model_up =
            str_to_vec(model_up).ok_or(Error::ConfigFileInvalid("invalid model_up value"))?;

        let model_right = options
            .get(&Yaml::String(String::from("model_right")))
            .ok_or(Error::ConfigFileInvalid("model_right value not found"))?
            .as_str()
            .ok_or(Error::ConfigFileInvalid("expected mode_right to be a str"))?;
        let model_right =
            str_to_vec(model_right).ok_or(Error::ConfigFileInvalid("invalid model_up value"))?;

        let model_forward = options
            .get(&Yaml::String(String::from("model_forward")))
            .ok_or(Error::ConfigFileInvalid("model_forward value not found"))?
            .as_str()
            .ok_or(Error::ConfigFileInvalid(
                "expected mode_forward to be a str",
            ))?;
        let model_forward =
            str_to_vec(model_forward).ok_or(Error::ConfigFileInvalid("invalid model_up value"))?;

        if model_right.cross(model_up) != model_forward
            && model_up.cross(model_right) != model_forward
        {
            return Err(Error::ConfigFileInvalid(
                "model_up, model_right, and model_forward, must form a valid cordinate system",
            ));
        }

        let model_to_world = {
            let to_model = math::Mat3::<f32>::from_cols(model_right, model_up, model_forward);

            // The transpose is equivalent to the inverse of a matrix when the matrix is orthonormal.
            let from_model = to_model.transposed();

            const INTO_WORLD: math::Mat3<f32> =
                math::Mat3::from_cols(ENGINE_RIGHT, ENGINE_UP, ENGINE_FORWARDS);

            from_model.mul(&INTO_WORLD)
        };

        Ok(Settings {
            _default_camera: default_camera,
            fov_y: fov_y as f32,
            mouse_sensitivity: mouse_sensitivity / 10.0,
            derive_normals,
            model_to_world,
            bindings: bindings.into_boxed_slice(),
        })
    }
}
