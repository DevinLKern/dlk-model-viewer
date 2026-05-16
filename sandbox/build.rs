use std::{io::BufWriter, io::Write, path::PathBuf, str::FromStr};

use obj_mtl::{Primitive, Vertex, VertexNormal};

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

    // (position, normal)
    let mut vertices = Vec::<(Vertex, VertexNormal)>::new();
    let mut indices = std::collections::HashMap::<obj_mtl::VtnIndex, usize>::new();

    let triangles = arrow_shape.get_primitives().filter_map(|p| match p {
        &Primitive::Triangle { v0, v1, v2 } => Some((v0, v1, v2)),
        _ => None,
    });
    writeln!(w, "#[allow(unused)]").unwrap();
    writeln!(w, "const ARROW_INDICES: &[u32] = &[").unwrap();
    for (v0, v1, v2) in triangles {
        let verts = [v0, v1, v2];
        for v in verts {
            let index = indices.entry(v).or_insert_with(|| {
                let pos = arrow_data.vs[v.v];
                let nor = arrow_data.vns[v.vn.unwrap()];
                let index = vertices.len();
                vertices.push((pos, nor));
                index
            });
            writeln!(w, "    {},", index).unwrap();
        }
    }
    writeln!(w, "];").unwrap();

    writeln!(w, "#[allow(unused)]").unwrap();
    writeln!(w, "const ARROW_VERTICES: &[[f32; 3]] = &[").unwrap();
    for v in vertices.iter() {
        let pos = v.0;
        writeln!(w, "    [{:?}, {:?}, {:?}],", pos.x, pos.y, pos.z).unwrap();
    }
    writeln!(w, "];").unwrap();

    writeln!(w, "#[allow(unused)]").unwrap();
    writeln!(w, "const ARROW_NORMALS: &[[f32; 3]] = &[").unwrap();
    for v in vertices {
        let nor = v.1;
        writeln!(w, "    [{:?}, {:?}, {:?}],", nor.x, nor.y, nor.z).unwrap();
    }
    writeln!(w, "];").unwrap();
}
