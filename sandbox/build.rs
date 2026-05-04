use std::{io::BufWriter, io::Write, path::PathBuf, str::FromStr};

use obj_mtl::Primitive;

fn main() {
    println!("cargo:rerun-if-changed=../files/models/arrow.obj");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let path = out_dir.join("arrow.rs");
    let file = std::fs::File::create(path).unwrap();
    let mut w = BufWriter::new(file);

    let arrow_path = PathBuf::from_str("../files")
        .unwrap()
        .join("models")
        .join("arrow.obj");

    let arrow_data = obj_mtl::ObjScene::from_file(&arrow_path).unwrap();

    let mut arrow_shapes = arrow_data.get_shapes();

    let arrow_shape = arrow_shapes.next().unwrap();

    writeln!(w, "#[allow(unused)]").unwrap();
    writeln!(w, "const ARROW_VERTICES: &[[f32; 3]] = &[").unwrap();
    let triangles = arrow_shape.get_primitives().filter_map(|p| match p {
        &Primitive::Triangle { v0, v1, v2 } => Some((v0, v1, v2)),
        _ => None,
    });
    for (v0, v1, v2) in triangles {
        let a = [
            arrow_data.vs[v0.v],
            arrow_data.vs[v1.v],
            arrow_data.vs[v2.v],
        ];
        for v in a {
            writeln!(w, "    [{:?}, {:?}, {:?}],", v.x, v.y, v.z).unwrap();
        }
    }
    writeln!(w, "];").unwrap();

    writeln!(w, "#[allow(unused)]").unwrap();
    writeln!(w, "const ARROW_NORMALS: &[[f32; 3]] = &[").unwrap();
    let triangles = arrow_shape.get_primitives().filter_map(|p| match p {
        &Primitive::Triangle { v0, v1, v2 } => Some((v0, v1, v2)),
        _ => None,
    });
    for (v0, v1, v2) in triangles {
        let a = [
            arrow_data.vns[v0.vn.unwrap()],
            arrow_data.vns[v1.vn.unwrap()],
            arrow_data.vns[v2.vn.unwrap()],
        ];
        for v in a {
            writeln!(w, "    [{:?}, {:?}, {:?}],", v.x, v.y, v.z).unwrap();
        }
    }
    writeln!(w, "];").unwrap();

}
