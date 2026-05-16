use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process::Command,
};

use spirv::TypeInfo;

fn get_type_name(type_info: &spirv::TypeInfo) -> String {
    match type_info {
        TypeInfo::Int {
            name,
            width,
            signed,
        } => match (width, signed) {
            (16, true) => String::from("i16"),
            (16, false) => String::from("u16"),
            (32, true) => String::from("i32"),
            (32, false) => String::from("u32"),
            (64, true) => String::from("i64"),
            (64, false) => String::from("u64"),
            _ => panic!("Int{{ {} {} {} }} not suppoted!", name, width, signed),
        },
        TypeInfo::Float { name, width } => match width {
            16 => String::from("f16"),
            32 => String::from("f32"),
            64 => String::from("f64"),
            _ => panic!("Float{{ {} {} }} not supported!", name, width),
        },
        TypeInfo::Vec {
            component_type,
            component_count,
            ..
        } => {
            format!("[{}; {}]", get_type_name(component_type), component_count)
        }
        TypeInfo::Mat {
            col_type,
            col_count,
            ..
        } => {
            format!("[{}; {}]", get_type_name(col_type), col_count)
        }
        TypeInfo::Array {
            element_type,
            element_count,
        } => {
            let element_type_name = get_type_name(&element_type);

            format!("[{}; {}]", element_type_name, element_count)
        }
        TypeInfo::Struct { name, .. } => name.to_string(),
        TypeInfo::RuntimeArray { .. } => "()".into(),
        _ => panic!("Type not supported! {:?}", type_info),
    }
}

fn type_info_to_rust(type_info: &spirv::TypeInfo) -> String {
    match type_info {
        TypeInfo::Struct { name, members, .. } => {
            for m in members.iter() {
                // TODO: field_type can be of type RutimeArray,
                // in which case the size will be unknown at build time.
                // This system should account for that possibility.
                println!(
                    "field_name: {}, field_offset: {}, field_size: {}",
                    m.field_name,
                    m.field_offset,
                    m.field_type.calc_size().unwrap()
                );
            }
            println!("\n");
            let mut res = format!("{} {{", name);
            let mut byte_count = 0;
            for (i, m) in members.iter().enumerate() {
                println!(
                    "field_name: {}, field_offset: {}, byte_count: {}",
                    m.field_name, m.field_offset, byte_count
                );
                if byte_count < m.field_offset {
                    let pad_size = m.field_offset - byte_count;
                    println!("adding padding! {}", pad_size);
                    let s = format!("pub _pad{}: [u8; {}], ", i, pad_size);
                    res.push_str(&s);
                }
                byte_count = m.field_offset + m.field_type.calc_size().unwrap();
                let x = format!("pub {}: {}, ", m.field_name, get_type_name(&m.field_type));
                res.push_str(&x);
            }
            res.push_str("}");
            res
        }
        _ => {
            println!("WARNING: {:?} not supported!", type_info);
            "()".into()
        }
    }
}

fn generate_struct_types(
    variable_types_path: &PathBuf,
    spv_modules: &[spirv::Module],
) -> Result<(), io::Error> {
    let variable_types_file = File::create(variable_types_path)?;
    let mut w = BufWriter::new(variable_types_file);

    let mut all_vars = HashMap::<Box<str>, spirv::TypeInfo>::new();

    for module in spv_modules.iter() {
        let type_infos = module.get_struct_types();

        for info in type_infos.into_iter() {
            let ty_info = match &info {
                TypeInfo::Pointer { ptr_type } => ptr_type,
                _ => &info,
            };
            let (name, _) = match ty_info {
                TypeInfo::Struct { name, .. } => (name, ty_info),
                _ => continue,
            };

            if let Some(ty_info) = all_vars.get(name) {
                if ty_info != &info {
                    panic!("Inconsistent type defintion for {}", name);
                }
            } else {
                all_vars.insert(name.clone(), info);
            }
        }

        let inputs = module.get_inputs();
        writeln!(w, "#[repr(C)]")?;
        writeln!(w, "pub struct {}Vertex {{", module.name)?;
        for info in inputs {
            let ty_str = if let TypeInfo::Pointer { ptr_type } = info.type_info {
                get_type_name(&ptr_type)
            } else {
                get_type_name(&info.type_info)
            };

            writeln!(w, "    pub {}: {},", info.name, ty_str)?;
        }
        writeln!(w, "}}")?;
    }

    for (_, type_info) in all_vars {
        writeln!(w, "#[repr(C)]")?;
        writeln!(w, "#[derive(Clone, Copy)]")?;
        writeln!(w, "pub struct {}", type_info_to_rust(&type_info))?;
    }

    Ok(())
}

fn generate_shader_paths(shader_paths_path: &PathBuf, paths: &[PathBuf]) -> Result<(), io::Error> {
    let shader_paths_file = File::create(shader_paths_path)?;
    let mut w = BufWriter::new(shader_paths_file);

    for path in paths {
        let prefix = path
            .file_prefix()
            .expect(format!("No file name for: {:?}", path).as_str())
            .to_ascii_uppercase();
        let extension = path
            .extension()
            .expect(format!("No file extension for: {:?}", path).as_str())
            .to_ascii_uppercase();

        writeln!(w, "#[allow(unused)]")?;

        writeln!(
            w,
            "const {}_{}_PATH: &str = \"{}/{}.spv\";",
            extension.display(),
            prefix.display(),
            env!("CARGO_MANIFEST_DIR"),
            path.to_str().unwrap()
        )?;
    }

    Ok(())
}

fn to_snake_caps(s: &str) -> String {
    let mut res = String::new();

    let mut c1: Option<char> = None;
    for c2 in s.chars() {
        if let Some(c1) = c1 {
            if c1.is_lowercase() && c2.is_uppercase() {
                res.push('_');
            }
        }

        if c2.is_lowercase() {
            res.push(c2.to_ascii_uppercase())
        } else {
            res.push(c2);
        }

        c1 = Some(c2);
    }

    res
}

fn generate_entry_point_vars(
    entry_points_path: &PathBuf,
    modules: &[spirv::Module],
) -> Result<(), io::Error> {
    let shader_paths_file = File::create(entry_points_path)?;
    let mut w = BufWriter::new(shader_paths_file);

    for m in modules {
        let name = m
            .get_entry_points()
            .find_map(|s| match s.as_str() {
                "main" => Some(s),
                _ => None,
            })
            .expect("Could not find entry point \"main\" ");

        writeln!(
            w,
            "const ENTRY_POINT_NAME_{}: &str = \"{}\";",
            to_snake_caps(&m.name),
            name
        )?;
    }

    Ok(())
}

fn run_rustfmt_on(path: &Path) {
    let status = Command::new("rustfmt")
        .arg(path)
        // Optional but recommended for generated files:
        .arg("--emit")
        .arg("files")
        .status()
        .expect("Failed to spawn rustfmt");

    if !status.success() {
        panic!("rustfmt failed on {}", path.display());
    }
}

fn compile_shader(path: &Path) {
    let output_path = path.with_added_extension("spv");

    let status = Command::new("glslc")
        .arg(path)
        .arg("-o")
        .arg(&output_path)
        .status()
        .expect("failed to execute glslc");

    if !status.success() {
        panic!("shader compilation failed for {}", path.display());
    }
}

fn main() {
    let shader_paths = [
        PathBuf::from("shaders")
            .join("shader")
            .with_added_extension("frag"),
        PathBuf::from("shaders")
            .join("shader")
            .with_added_extension("vert"),
    ];

    for path in &shader_paths {
        println!("cargo:rerun-if-changed={}", path.display());
        compile_shader(path);
    }

    let spv_modules: Box<[spirv::Module]> = shader_paths
        .iter()
        .map(|path| {
            let spv_path = path.with_added_extension("spv");
            spirv::Module::from_file(&spv_path).unwrap_or_else(|e| {
                panic!("could not parse spv file {}: {:?}", spv_path.display(), e)
            })
        })
        .collect();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let variable_types_path = out_dir.join("variable_types.rs");
    generate_struct_types(&variable_types_path, &spv_modules).unwrap();
    run_rustfmt_on(&variable_types_path);

    let shader_paths_path = out_dir.join("shader_paths.rs");
    generate_shader_paths(&shader_paths_path, &shader_paths).unwrap();
    run_rustfmt_on(&shader_paths_path);

    let entry_point_names_path = out_dir.join("entry_points.rs");
    generate_entry_point_vars(&entry_point_names_path, &spv_modules).unwrap();
    run_rustfmt_on(&entry_point_names_path);
}
