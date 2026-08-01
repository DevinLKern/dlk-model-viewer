use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use crate::ObjTokenizer;

pub type Float = f32;
pub type Index = u32;

#[derive(Copy, Clone, Debug)]
pub struct Vertex {
    pub x: Float,
    pub y: Float,
    pub z: Float,
    pub w: Float,
}

#[derive(Default, Copy, Clone, Debug)]
pub struct VertexTexture {
    pub u: Float,
    pub v: Float,
    pub w: Option<Float>,
}

#[derive(Copy, Clone, Default, Debug)]
pub struct VertexNormal {
    pub x: Float,
    pub y: Float,
    pub z: Float,
}

#[derive(Eq, PartialEq, Hash, Copy, Clone, Default, Debug)]
pub struct VtnIndex {
    pub v: Index,
    pub vt: Option<Index>,
    pub vn: Option<Index>,
}

impl VtnIndex {
    fn adjustst_index(count: usize, index: Option<i64>) -> Option<Index> {
        let i = index?;

        if i == 0 {
            return None;
        }

        let i = if i < 0 { count as i64 + i } else { i - 1 };

        Some(i as Index)
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
    pub name: Option<Arc<str>>,
    // (index_of_material_name, shading_group, primitive)
    primitives: Box<[Primitive]>,
    // (material_name, first_primitive_index, primitive_count)
    pub material_ranges: Box<[(Arc<str>, usize, usize)]>,
    // (shading_group, first_primitive_index, primitive_count)
    shading_group_ranges: Box<[(u32, usize, usize)]>,
}

impl Shape {
    pub fn primitives(&self) -> impl Iterator<Item = &Primitive> {
        self.primitives.iter()
    }
}

#[allow(unused)]
pub struct ObjScene {
    pub vs: Box<[Vertex]>,
    pub vts: Box<[VertexTexture]>,
    pub vns: Box<[VertexNormal]>,
    material_file_names: HashSet<Arc<str>>,
    material_names: HashSet<Arc<str>>,
    shapes: Box<[Shape]>,
}

pub struct ShapeIterator {
    tokenizer: ObjTokenizer,

    pub material_file_names: HashSet<Arc<str>>,
    pub material_names: HashSet<Arc<str>>,
    vs: Vec<Vertex>,
    vts: Vec<VertexTexture>,
    vns: Vec<VertexNormal>,

    shapes: VecDeque<Shape>,

    cur_shape_name: Option<Arc<str>>,
    cur_shape_primitives: Vec<Primitive>,
    cur_material_ranges: Vec<(Arc<str>, usize, usize)>,
    cur_shading_group_ranges: Vec<(u32, usize, usize)>,
}

impl ShapeIterator {
    pub fn new(path: &Path) -> crate::Result<Self> {
        let tokenizer = ObjTokenizer::from_path(path)?;

        const INITIAL_CAPACITY: usize = 256;

        Ok(Self {
            tokenizer,

            material_file_names: HashSet::new(),
            material_names: HashSet::new(),

            vs: Vec::with_capacity(INITIAL_CAPACITY),
            vts: Vec::with_capacity(INITIAL_CAPACITY),
            vns: Vec::with_capacity(INITIAL_CAPACITY),

            shapes: VecDeque::new(),

            cur_shape_name: None,
            cur_material_ranges: Vec::new(),
            cur_shading_group_ranges: Vec::new(),
            cur_shape_primitives: Vec::with_capacity(INITIAL_CAPACITY),
        })
    }
    fn flush(&mut self) -> Option<Shape> {
        let has_nothing = self.cur_shape_name.is_none() && self.cur_shape_primitives.is_empty();
        if has_nothing {
            self.cur_shape_name.take();
            self.cur_shape_primitives.clear();
            return None;
        }

        if let Some((_, first_index, element_count)) = self.cur_material_ranges.last_mut() {
            *element_count = self.cur_shape_primitives.len() - *first_index;
        }
        if self
            .cur_material_ranges
            .last()
            .map(|(_, _, c)| *c == 0)
            .unwrap_or(false)
        {
            self.cur_material_ranges.pop();
        }

        if let Some((_, first_index, element_count)) = self.cur_shading_group_ranges.last_mut() {
            *element_count = self.cur_shape_primitives.len() - *first_index;
        }
        if self
            .cur_shading_group_ranges
            .last()
            .map(|(_, _, c)| *c == 0)
            .unwrap_or(false)
        {
            self.cur_shading_group_ranges.pop();
        }

        Some(Shape {
            name: self.cur_shape_name.take(),
            primitives: self.cur_shape_primitives.drain(..).collect(),
            material_ranges: self.cur_material_ranges.drain(..).collect(),
            shading_group_ranges: self.cur_shading_group_ranges.drain(..).collect(),
        })
    }
    /// Returns None on EOF
    fn process_next_token(&mut self) -> Option<crate::Result<()>> {
        use crate::obj_tokenizer::ObjToken;

        let token = match self.tokenizer.next_token() {
            Some(t) => t,
            None => {
                if let Some(shape) = self.flush() {
                    if !shape.primitives.is_empty() {
                        self.shapes.push_back(shape);
                    }
                }
                return None;
            }
        };

        let token = match token {
            Ok(t) => t,
            Err(e) => return Some(Err(e)),
        };

        match token {
            ObjToken::MtlFile(file_name) => {
                self.material_file_names.insert(file_name.into());
            }
            ObjToken::UseMaterial(material_name) => {
                let material_name: Arc<str> = material_name.clone().into();
                let material_name = if let Some(s) = self.material_names.get(&material_name) {
                    s.clone()
                } else {
                    self.material_names.insert(material_name.clone());
                    material_name
                };

                if let Some((_, first_index, element_count)) = self.cur_material_ranges.last_mut() {
                    *element_count = self.cur_shape_primitives.len() - *first_index;
                }

                let last_is_different = self
                    .cur_material_ranges
                    .last()
                    .map(|(name, ..)| name != &material_name)
                    .unwrap_or(true);
                if last_is_different {
                    self.cur_material_ranges.push((
                        material_name,
                        self.cur_shape_primitives.len(),
                        0,
                    ));
                }
            }
            ObjToken::Shading(shading_group) => {
                if let Some((_, first_index, element_count)) =
                    self.cur_shading_group_ranges.last_mut()
                {
                    *element_count = self.cur_shape_primitives.len() - *first_index;
                }

                let last_is_different = self
                    .cur_shading_group_ranges
                    .last()
                    .map(|(group, ..)| group != &shading_group)
                    .unwrap_or(true);
                if last_is_different {
                    self.cur_shading_group_ranges.push((
                        shading_group,
                        self.cur_shape_primitives.len(),
                        0,
                    ));
                }
            }
            ObjToken::Face(face_vertices) => {
                let primitive = match face_vertices.as_ref() {
                    &[v0] => Primitive::Point(VtnIndex::from_raw_index(
                        self.vs.len(),
                        self.vts.len(),
                        self.vns.len(),
                        v0,
                    )),
                    &[v0, v1, v2] => Primitive::Triangle {
                        v0: VtnIndex::from_raw_index(
                            self.vs.len(),
                            self.vts.len(),
                            self.vns.len(),
                            v0,
                        ),
                        v1: VtnIndex::from_raw_index(
                            self.vs.len(),
                            self.vts.len(),
                            self.vns.len(),
                            v1,
                        ),
                        v2: VtnIndex::from_raw_index(
                            self.vs.len(),
                            self.vts.len(),
                            self.vns.len(),
                            v2,
                        ),
                    },
                    vertices => Primitive::Polygon(
                        vertices
                            .into_iter()
                            .map(|v| {
                                VtnIndex::from_raw_index(
                                    self.vs.len(),
                                    self.vts.len(),
                                    self.vns.len(),
                                    *v,
                                )
                            })
                            .collect(),
                    ),
                };

                self.cur_shape_primitives.push(primitive);
            }
            ObjToken::Line(line_vertices) => {
                let primitive = Primitive::Line(
                    line_vertices
                        .iter()
                        .map(|v| {
                            VtnIndex::from_raw_index(
                                self.vs.len(),
                                self.vts.len(),
                                self.vns.len(),
                                *v,
                            )
                        })
                        .collect(),
                );
                self.cur_shape_primitives.push(primitive);
            }
            ObjToken::Object(object_name) => {
                let shape = self.flush();
                self.cur_shape_name = Some(object_name.into());
                if let Some(s) = shape {
                    self.shapes.push_back(s);
                }
            }
            ObjToken::Group(group_name) => {
                let shape = self.flush();
                self.cur_shape_name = Some(group_name.into());
                if let Some(s) = shape {
                    self.shapes.push_back(s);
                }
            }
            ObjToken::V { x, y, z, w } => {
                self.vs.push(Vertex {
                    x,
                    y,
                    z,
                    w: w.unwrap_or(1.0),
                });
            }
            ObjToken::Vt { u, v, w } => {
                self.vts.push(VertexTexture { u, v, w });
            }
            ObjToken::Vn { x, y, z } => {
                self.vns.push(VertexNormal { x, y, z });
            }
            _ => {}
        }

        Some(Ok(()))
    }
    pub fn get_vertex(&mut self, index: Index) -> Option<&Vertex> {
        while self.vs.len() <= index as usize {
            if let Err(_) = self.process_next_token()? {
                return None;
            }
        }
        self.vs.get(index as usize)
    }
    pub fn get_vertex_normal(&mut self, index: Index) -> Option<&VertexNormal> {
        while self.vns.len() <= index as usize {
            if let Err(_) = self.process_next_token()? {
                return None;
            }
        }
        self.vns.get(index as usize)
    }
    pub fn get_vertex_texture(&mut self, index: Index) -> Option<&VertexTexture> {
        while self.vts.len() <= index as usize {
            if let Err(_) = self.process_next_token()? {
                return None;
            }
        }
        self.vts.get(index as usize)
    }
    pub fn next_shape(&mut self) -> Option<Shape> {
        while let None = self.shapes.front() {
            match self.process_next_token() {
                Some(Ok(())) => {}
                Some(Err(_)) => return None,
                None => break,
            }
        }
        self.shapes.pop_front()
    }
}

impl ObjScene {
    pub fn from_file(path: &Path) -> crate::Result<ObjScene> {
        let mut shape_iterator = ShapeIterator::new(path)?;
        let mut shapes = Vec::<Shape>::with_capacity(8);
        while let Some(shape) = shape_iterator.next_shape() {
            shapes.push(shape);
        }

        Ok(ObjScene {
            vs: shape_iterator.vs.into_boxed_slice(),
            vts: shape_iterator.vts.into_boxed_slice(),
            vns: shape_iterator.vns.into_boxed_slice(),
            material_file_names: shape_iterator.material_file_names.clone(),
            material_names: shape_iterator.material_names,
            shapes: shapes.into_boxed_slice(),
        })
    }
    pub fn shapes(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter()
    }
}
