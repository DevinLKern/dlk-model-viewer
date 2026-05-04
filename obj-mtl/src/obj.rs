use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Copy, Clone)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

pub struct VertexTexture {
    pub u: f64,
    pub v: f64,
    pub w: Option<f64>,
}

#[derive(Copy, Clone)]
pub struct VertexNormal {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct VtnIndex {
    pub v: usize,
    pub vt: Option<usize>,
    pub vn: Option<usize>,
}

impl VtnIndex {
    fn adjustst_index(count: usize, index: Option<i64>) -> Option<usize> {
        let i = index?;

        if i == 0 {
            return None;
        }

        let i = if i < 0 { count as i64 + i } else { i - 1 };

        Some(i as usize)
    }
    pub fn from_raw_index(
        v_count: usize,
        vt_count: usize,
        vn_count: usize,
        index: crate::VtnIndexRaw,
    ) -> Self {
        Self {
            v: Self::adjustst_index(v_count, Some(index.v)).unwrap(),
            vt: Self::adjustst_index(vt_count, index.vt),
            vn: Self::adjustst_index(vn_count, index.vn),
        }
    }
}

#[allow(unused)]
const OBJ_SHADING_GROUP_FLAT: u32 = 0;

pub enum Primitive {
    Point(VtnIndex),
    Line(Box<[VtnIndex]>),
    Triangle {
        v0: VtnIndex,
        v1: VtnIndex,
        v2: VtnIndex,
    },
    Polygon(Box<[VtnIndex]>),
}

#[allow(unused)]
pub struct Shape {
    pub name: Option<Box<str>>,
    pub materials: Box<[Box<str>]>,
    // (index_of_material_name, shading_group, primitive)
    primitives: Box<[(Option<usize>, u32, Primitive)]>,
}

impl Shape {
    pub fn get_primitives(&self) -> impl Iterator<Item = &Primitive> {
        self.primitives.iter().map(|(_, _, p)| p)
    }
}

#[allow(unused)]
pub struct ObjScene {
    pub vs: Box<[Vertex]>,
    pub vts: Box<[VertexTexture]>,
    pub vns: Box<[VertexNormal]>,
    material_files: Box<[PathBuf]>,
    shapes: Box<[Shape]>,
}

fn flush_shape(
    shapes: &mut Vec<Shape>,
    cur_shape_name: &mut Option<Box<str>>,
    cur_shape_materials: &mut Vec<Box<str>>,
    cur_shape_primitives: &mut Vec<(Option<usize>, u32, Primitive)>,
) {
    let has_nothing = cur_shape_name.is_none()
        && cur_shape_materials.is_empty()
        && cur_shape_primitives.is_empty();
    if has_nothing {
        cur_shape_name.take();
        cur_shape_materials.clear();
        cur_shape_primitives.clear();
        return;
    }

    shapes.push(Shape {
        name: cur_shape_name.take(),
        materials: cur_shape_materials.drain(..).collect(),
        primitives: cur_shape_primitives.drain(..).collect(),
    });
}

impl ObjScene {
    pub fn from_file(path: &Path) -> crate::Result<ObjScene> {
        let mut shapes = Vec::<Shape>::new();
        let mut material_names = Vec::<Box<str>>::new();
        let mut material_indexes = HashMap::<Box<str>, usize>::new();
        let mut material_files = Vec::<PathBuf>::new();
        let mut vs = Vec::<Vertex>::with_capacity(64);
        let mut vts = Vec::<VertexTexture>::with_capacity(64);
        let mut vns = Vec::<VertexNormal>::with_capacity(64);

        let mut tokenizer = crate::ObjTokenizer::from_path(path)?;

        let mut cur_material_index: Option<usize> = None;
        let mut cur_shading_group: u32 = 0;
        let mut cur_shape_name: Option<Box<str>> = None;
        let mut cur_shape_materials = Vec::<Box<str>>::new();
        // (material_index, shading_group, primitive)
        let mut cur_shape_primitives = Vec::<(Option<usize>, u32, Primitive)>::new();

        while let Some(token) = tokenizer.next_token() {
            use crate::ObjToken::*;

            match token? {
                MtlFile(file_path_as_str) => {
                    // Unwrap is ok because error is of type Infallible
                    let normalized = file_path_as_str.replace('\\', "/");
                    let file_path = PathBuf::from_str(&normalized).unwrap();
                    material_files.push(file_path);
                }
                UseMaterial(material_name) => {
                    // only push if the material name is not in material_names
                    let index = material_indexes
                        .entry(material_name.clone())
                        .or_insert_with(|| {
                            let i = material_names.len();
                            cur_shape_materials.push(material_name.clone());
                            material_names.push(material_name);
                            i
                        });

                    cur_material_index = Some(*index);
                }
                Shading(s) => {
                    cur_shading_group = s;
                }
                Object(object_name) => {
                    flush_shape(
                        &mut shapes,
                        &mut cur_shape_name,
                        &mut cur_shape_materials,
                        &mut cur_shape_primitives,
                    );
                    cur_shape_name = Some(object_name);
                }
                Group(group_name) => {
                    flush_shape(
                        &mut shapes,
                        &mut cur_shape_name,
                        &mut cur_shape_materials,
                        &mut cur_shape_primitives,
                    );
                    cur_shape_name = Some(group_name);
                }
                V { x, y, z, w } => {
                    vs.push(Vertex {
                        x,
                        y,
                        z,
                        w: w.unwrap_or(1.0),
                    });
                }
                Vt { u, v, w } => {
                    // not sure about the unwrap_or_default part
                    vts.push(VertexTexture { u, v, w });
                }
                Vn { x, y, z } => {
                    vns.push(VertexNormal { x, y, z });
                }
                Face(indices) => {
                    let primitive = match indices.as_ref() {
                        &[v0] => Primitive::Point(VtnIndex::from_raw_index(
                            vs.len(),
                            vts.len(),
                            vns.len(),
                            v0,
                        )),
                        &[v0, v1, v2] => Primitive::Triangle {
                            v0: VtnIndex::from_raw_index(vs.len(), vts.len(), vns.len(), v0),
                            v1: VtnIndex::from_raw_index(vs.len(), vts.len(), vns.len(), v1),
                            v2: VtnIndex::from_raw_index(vs.len(), vts.len(), vns.len(), v2),
                        },
                        vertices => Primitive::Polygon(
                            vertices
                                .into_iter()
                                .map(|v| {
                                    VtnIndex::from_raw_index(vs.len(), vts.len(), vns.len(), *v)
                                })
                                .collect(),
                        ),
                    };

                    cur_shape_primitives.push((
                        cur_material_index.clone(),
                        cur_shading_group,
                        primitive,
                    ));
                }
                Line(vertices) => {
                    let primitive = Primitive::Line(
                        vertices
                            .iter()
                            .map(|v| VtnIndex::from_raw_index(vs.len(), vts.len(), vns.len(), *v))
                            .collect(),
                    );
                    cur_shape_primitives.push((
                        cur_material_index.clone(),
                        cur_shading_group,
                        primitive,
                    ));
                }
                _ => {}
            }
        }

        flush_shape(
            &mut shapes,
            &mut cur_shape_name,
            &mut cur_shape_materials,
            &mut cur_shape_primitives,
        );

        Ok(ObjScene {
            vs: vs.into_boxed_slice(),
            vts: vts.into_boxed_slice(),
            vns: vns.into_boxed_slice(),
            material_files: material_files.into_boxed_slice(),
            shapes: shapes.into_boxed_slice(),
        })
    }
    pub fn get_shapes(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter()
    }
}
