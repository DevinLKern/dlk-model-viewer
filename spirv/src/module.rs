use crate::{Error, Result};
use std::io::Read;

struct RawInstruction {
    opcode: u32,
    operands: Box<[u32]>,
}

pub struct Module {
    pub name: Box<str>,
    instructions: Vec<RawInstruction>,
}

#[derive(Debug, PartialEq, PartialOrd, Ord, Eq)]
#[allow(unused)]
pub enum TypeInfo {
    Void,
    Bool,
    Int {
        name: Box<str>,
        width: u32,
        signed: bool,
    },
    Float {
        name: Box<str>,
        width: u32,
    },
    Vec {
        name: Box<str>,
        component_type: Box<Self>,
        component_count: u32,
    },
    Mat {
        name: Box<str>,
        col_type: Box<Self>,
        col_count: u32,
    },
    Struct {
        name: Box<str>,
        members: Box<[StructMemberInfo]>,
    },
    Pointer {
        ptr_type: Box<Self>,
    },
    Image {
        sampled_type: Box<Self>,
        format: u32,
        depth: u32,
        dimentionality: u32,
        arrayed: bool,
        multisampled: bool,
        sampled: u32,
    },
    Sampler,
    SampledImage {
        image_type: Box<Self>,
    },
    Array {
        element_type: Box<Self>,
        element_count: u32,
    },
    RuntimeArray {
        element_type: Box<Self>,
    },
}

#[derive(Debug, Eq, PartialEq, PartialOrd, Ord)]
#[allow(unused)]
pub struct StructMemberInfo {
    pub field_type: TypeInfo,
    pub field_offset: u32,
    pub field_name: Box<str>,
}

#[allow(dead_code)]
impl TypeInfo {
    pub fn calc_size(&self) -> Option<u32> {
        match self {
            TypeInfo::Bool => Some(1),
            TypeInfo::Int { width, .. } => Some(*width / 8),
            TypeInfo::Float { width, .. } => Some(*width / 8),
            TypeInfo::Vec {
                component_type,
                component_count,
                ..
            } => {
                let component_size = component_type.calc_size()?;
                Some(component_size * component_count)
            }
            TypeInfo::Mat {
                col_type,
                col_count,
                ..
            } => {
                let col_size = col_type.calc_size()?;
                Some(col_size * col_count)
            }
            TypeInfo::Struct { members, .. } => {
                let mut last_member: Option<&StructMemberInfo> = None;
                for member in members.iter() {
                    if let Some(lm) = last_member {
                        if member.field_offset > lm.field_offset {
                            last_member = Some(member);
                        }
                    } else {
                        last_member = Some(member);
                    }
                }

                if last_member.is_none() {
                    return Some(0);
                }
                let last_member = last_member.unwrap();

                Some(last_member.field_offset + last_member.field_type.calc_size()?)
            }
            TypeInfo::Array {
                element_type,
                element_count,
            } => {
                let element_size = element_type.calc_size()?;

                Some(element_size * element_count)
            }
            TypeInfo::RuntimeArray { .. } => {
                println!("WARNING: runtime array!");
                Some(0)
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct UniformInfo {
    pub set: u32,
    pub binding: u32,
    pub ty: TypeInfo,
    pub storage_class: u32,
    pub descriptor_count: u32,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ShaderIoInfo {
    pub location: u32,
    pub name: Box<str>,
    pub type_info: TypeInfo,
}

#[allow(unused)]
impl Module {
    pub fn from_code(name: Box<str>, shader_code: &[u8]) -> Result<Self> {
        if shader_code.len() < 4 * 5 || shader_code.len() % 4 != 0 {
            return Err(Error::InvalidFileLength(shader_code.len()));
        }

        let mut chunks = shader_code.chunks_exact(4);

        let magic = u32::from_le_bytes(chunks.next().unwrap().try_into().unwrap());
        if magic != crate::MAGIC_NUMBER {
            return Err(Error::IncorrectMagicWord(magic));
        }

        // TODO: look up if versions are backwards compatible
        let version = u32::from_le_bytes(chunks.next().unwrap().try_into().unwrap());
        if version > crate::SPIRV_VERSION {
            return Err(Error::InvalidVersion((version, crate::SPIRV_VERSION)));
        }

        let _generator = chunks.next().unwrap();
        let _bound = chunks.next().unwrap();
        let _reserved = chunks.next().unwrap();

        let mut instructions = Vec::<RawInstruction>::new();

        while let Some(word) = chunks.next() {
            let first_word = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
            let word_count = (first_word >> 16) as usize;
            let opcode = first_word & 0xFFFF;

            if word_count == 0 {
                panic!("Word count should not be 0");
            }

            let mut operands = Vec::with_capacity(word_count - 1);
            for _ in 1..word_count {
                let operand = chunks.next().unwrap();
                let operand = u32::from_le_bytes([operand[0], operand[1], operand[2], operand[3]]);
                operands.push(operand);
            }
            let operands = operands.into_boxed_slice();

            instructions.push(RawInstruction { opcode, operands });
        }

        Ok(Module { name, instructions })
    }
    pub fn from_file(shader_path: &std::path::Path) -> Result<Self> {
        let mut file = std::fs::File::open(shader_path).map_err(|e| Error::Io(e))?;

        let mut data = Vec::<u8>::new();

        let _ = file.read_to_end(&mut data).map_err(|e| Error::Io(e))?;

        let capitalize_first = |input: &str| -> String {
            let lowercased = input.to_lowercase();
            let mut chars = lowercased.chars();

            match chars.next() {
                None => String::new(),
                Some(first_char) => first_char.to_uppercase().collect::<String>() + chars.as_str(),
            }
        };

        let path_str = shader_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let parts: Vec<&str> = path_str.split(".").collect();
        let mut p1 = capitalize_first(parts[0]);
        let p2 = capitalize_first(parts[1]);

        p1.push_str(&p2);

        Self::from_code(p1.into_boxed_str(), data.as_slice())
    }
    fn get_variables(&self) -> impl Iterator<Item = &RawInstruction> {
        self.instructions
            .iter()
            .filter(|i| i.opcode == crate::OP_VARIABLE)
    }
    fn get_decorations(&self) -> impl Iterator<Item = &RawInstruction> {
        self.instructions
            .iter()
            .filter(|i| i.opcode == crate::OP_DECORATE)
    }
    fn get_types(&self) -> impl Iterator<Item = &RawInstruction> {
        self.instructions.iter().filter(|i| match i.opcode {
            crate::OP_TYPE_VOID
            | crate::OP_TYPE_INT
            | crate::OP_TYPE_FLOAT
            | crate::OP_TYPE_VECTOR
            | crate::OP_TYPE_MATRIX
            | crate::OP_TYPE_STRUCT
            | crate::OP_TYPE_POINTER
            | crate::OP_TYPE_ARRAY
            | crate::OP_TYPE_RUNTIME_ARRAY
            | crate::OP_TYPE_IMAGE
            | crate::OP_TYPE_SAMPLER
            | crate::OP_TYPE_SAMPLED_IMAGE => true,
            _ => false,
        })
    }
    fn parse_string_literal(operands: &[u32]) -> String {
        let mut name_bytes = Vec::new();
        'outer: for j in 1..operands.len() {
            let word_bytes = operands[j].to_le_bytes();
            for &byte in &word_bytes {
                if byte == 0 {
                    break 'outer;
                }
                name_bytes.push(byte);
            }
        }
        String::from_utf8_lossy(&name_bytes).into_owned()
    }
    fn get_type_name_from_id(&self, type_id: u32) -> Option<String> {
        self.instructions.iter().find_map(|i| {
            if i.opcode != crate::OP_NAME || i.operands[0] != type_id {
                return None;
            }
            Some(Self::parse_string_literal(&i.operands))
        })
    }
    fn get_type_from_id(&self, type_id: u32) -> Result<TypeInfo> {
        for i in self.instructions.iter() {
            if i.operands.len() == 0 || i.operands[0] != type_id {
                continue;
            }
            let structure_type_id = i.operands[0];
            let uniform_type_info: TypeInfo = match i.opcode {
                crate::OP_TYPE_VOID => TypeInfo::Void,
                crate::OP_TYPE_BOOL => TypeInfo::Bool,
                crate::OP_TYPE_INT => {
                    let width = i.operands[1];
                    let signed = i.operands[2] == 1;
                    let default_name = match (width, signed) {
                        (32, true) => Some(String::from("int")),
                        (32, false) => Some(String::from("uint")),
                        _ => None,
                    };
                    let name = self
                        .get_type_name_from_id(i.operands[0])
                        .unwrap_or_else(|| default_name.unwrap())
                        .into_boxed_str();
                    TypeInfo::Int {
                        name,
                        width,
                        signed,
                    }
                }
                crate::OP_TYPE_FLOAT => {
                    let width = i.operands[1];
                    let default_name = match (width) {
                        32 => Some(String::from("float")),
                        64 => Some(String::from("double")),
                        _ => None,
                    };
                    let name = self
                        .get_type_name_from_id(i.operands[0])
                        .unwrap_or_else(|| default_name.unwrap())
                        .into_boxed_str();
                    TypeInfo::Float { name, width }
                }
                crate::OP_TYPE_VECTOR => {
                    let component_type_id = i.operands[1];
                    let component_type = Box::new(self.get_type_from_id(component_type_id)?);
                    let component_count = i.operands[2];
                    let default_name = format!("vec{}", component_count);
                    let name = self
                        .get_type_name_from_id(i.operands[0])
                        .unwrap_or(default_name)
                        .into_boxed_str();
                    TypeInfo::Vec {
                        name,
                        component_type,
                        component_count,
                    }
                }
                crate::OP_TYPE_MATRIX => {
                    let col_type_id = i.operands[1];
                    let col_type = Box::new(self.get_type_from_id(col_type_id)?);
                    let col_count = i.operands[2];
                    let default_name = format!("mat{}", col_count);
                    let name = self
                        .get_type_name_from_id(i.operands[0])
                        .unwrap_or(default_name)
                        .into_boxed_str();
                    TypeInfo::Mat {
                        name,
                        col_type,
                        col_count,
                    }
                }
                crate::OP_TYPE_STRUCT => {
                    let name = self
                        .get_type_name_from_id(i.operands[0])
                        .ok_or(Error::NameMissing(i.operands[0]))?
                        .into_boxed_str();

                    if i.operands.len() <= 1 {
                        TypeInfo::Struct {
                            name,
                            members: Box::new([]),
                        }
                    } else {
                        let members: Box<[StructMemberInfo]> = i
                            .operands
                            .iter()
                            .skip(1)
                            .enumerate()
                            .filter_map(|(member_index, member_type_id)| {
                                let field_name = self.instructions.iter().find_map(|d| {
                                    if d.opcode != crate::OP_MEMBER_NAME {
                                        return None;
                                    }
                                    if d.operands[0] != structure_type_id {
                                        return None;
                                    }
                                    if d.operands[1] as usize != member_index {
                                        return None;
                                    }
                                    Some(Self::parse_string_literal(&d.operands[1..]))
                                });

                                let field_type = self.get_type_from_id(*member_type_id);

                                let field_offset = self.instructions.iter().find_map(|d| {
                                    if d.opcode != crate::OP_MEMBER_DECORATE {
                                        return None;
                                    }
                                    let decoration = d.operands[2];
                                    if decoration != crate::DECORATION_OFFSET {
                                        return None;
                                    }
                                    if d.operands[0] != structure_type_id {
                                        return None;
                                    }
                                    if d.operands[1] as usize != member_index {
                                        return None;
                                    }
                                    Some(d.operands[3])
                                });

                                match (field_name, field_type, field_offset) {
                                    (Some(n), Ok(ty), Some(o)) => Some(StructMemberInfo {
                                        field_type: ty,
                                        field_offset: o,
                                        field_name: n.into_boxed_str(),
                                    }),
                                    _ => None,
                                }
                            })
                            .collect();

                        if members.len() < i.operands.len() - 1 {
                            return Err(Error::InvalidType); // TODO: Add an error type?
                        }

                        TypeInfo::Struct { name, members }
                    }
                }
                crate::OP_TYPE_POINTER => {
                    let ptr_type_id = i.operands[2];
                    let ptr_type = Box::new(self.get_type_from_id(ptr_type_id)?);

                    TypeInfo::Pointer { ptr_type }
                }
                crate::OP_TYPE_IMAGE => {
                    let image_type_id = i.operands[0];
                    let sampled_type = i.operands[1];
                    let dimentionality = i.operands[2];
                    let depth = i.operands[3];
                    let arrayed = i.operands[4];
                    let multisampled = i.operands[5];
                    let sampled = i.operands[6];
                    let format = i.operands[7];
                    // let access = i.operands[8];

                    TypeInfo::Image {
                        sampled_type: Box::new(self.get_type_from_id(sampled_type)?),
                        format,
                        dimentionality,
                        depth,
                        arrayed: arrayed == 1,
                        multisampled: multisampled == 1,
                        sampled,
                    }
                }
                crate::OP_TYPE_SAMPLER => TypeInfo::Sampler,
                crate::OP_TYPE_SAMPLED_IMAGE => {
                    let image_type_id = i.operands[1];
                    TypeInfo::SampledImage {
                        image_type: Box::new(self.get_type_from_id(image_type_id)?),
                    }
                }
                crate::OP_TYPE_ARRAY => {
                    let element_type_id = i.operands[1];
                    let element_count = i.operands[2];

                    TypeInfo::Array {
                        element_type: Box::new(self.get_type_from_id(element_type_id)?),
                        element_count,
                    }
                }
                crate::OP_TYPE_RUNTIME_ARRAY => {
                    let element_type_id = i.operands[1];
                    TypeInfo::RuntimeArray {
                        element_type: Box::new(self.get_type_from_id(element_type_id)?),
                    }
                }
                _ => {
                    continue;
                }
            };

            return Ok(uniform_type_info);
        }

        Err(Error::InvalidType)
    }
    fn descriptor_count_from_type(ty: &crate::TypeInfo) -> u32 {
        use crate::TypeInfo::*;

        match ty {
            Array {
                element_type,
                element_count,
            } => element_count * Self::descriptor_count_from_type(element_type),
            Pointer { ptr_type } => Self::descriptor_count_from_type(ptr_type),

            _ => 1,
        }
    }
    pub fn get_uniform_info(&self) -> Box<[UniformInfo]> {
        let mut uniforms = Vec::<UniformInfo>::new();
        for v in self.get_variables() {
            let variable_id = v.operands[1];
            let storage_class = v.operands[2];

            if storage_class != crate::STORAGE_CLASS_UNIFORM
                && storage_class != crate::STORAGE_CLASS_UNIFORM_CONSTANT
                && storage_class != crate::ACCESS_QUALIFIER_READ_ONLY
                && storage_class != crate::STORAGE_CLASS_IMAGE
            {
                continue;
            }

            let set = self.get_decorations().find_map(|d| {
                let id = d.operands[0];
                if id != variable_id {
                    return None;
                }

                let decoration = d.operands[1];
                if decoration != crate::DECORATION_DESCRIPTOR_SET {
                    return None;
                }

                Some(d.operands[2])
            });

            let binding = self.get_decorations().find_map(|d| {
                let id = d.operands[0];
                if id != variable_id {
                    return None;
                }

                let decoration = d.operands[1];
                if decoration != crate::DECORATION_BINDING {
                    return None;
                }

                Some(d.operands[2])
            });

            let variable_type_id = v.operands[0];

            let ty = self.get_type_from_id(variable_type_id);

            if let (Some(set), Some(binding), Ok(ty)) = (set, binding, ty) {
                let descriptor_count = Self::descriptor_count_from_type(&ty);
                uniforms.push(UniformInfo {
                    set,
                    binding,
                    ty,
                    storage_class,
                    descriptor_count,
                });
            } else {
                panic!("TODO: add error type");
            }
        }

        uniforms.into_boxed_slice()
    }
    pub fn get_inputs(&self) -> impl Iterator<Item = ShaderIoInfo> {
        self.instructions.iter().filter_map(|i| {
            if i.opcode != crate::OP_VARIABLE {
                return None;
            }

            let storage_class = i.operands[2];
            if storage_class != crate::STORAGE_CLASS_INPUT {
                return None;
            }

            let variable_type_id = i.operands[0];
            let variable_type_info = self.get_type_from_id(variable_type_id);
            if variable_type_info.is_err() {
                return None;
            }

            let variable_id = i.operands[1];
            let variable_name = self.get_type_name_from_id(variable_id);
            if variable_name.is_none() {
                return None;
            }

            let location = self.get_decorations().find_map(|d| {
                let target_id = d.operands[0];
                if target_id != variable_id {
                    return None;
                }

                let decoration = d.operands[1];
                if decoration != crate::DECORATION_LOCATION {
                    return None;
                }

                Some(d.operands[2])
            });
            if location.is_none() {
                return None;
            }

            Some(ShaderIoInfo {
                location: location.unwrap(),
                name: variable_name.unwrap().into_boxed_str(),
                type_info: variable_type_info.unwrap(),
            })
        })
    }
    pub fn get_variable_types(&self) -> impl Iterator<Item = TypeInfo> {
        self.get_types().map(|ty| {
            let type_id = ty.operands[0];
            self.get_type_from_id(type_id).unwrap()
        })
    }
    pub fn get_struct_types(&self) -> impl Iterator<Item = TypeInfo> {
        self.instructions.iter().filter_map(|ty| {
            if ty.opcode == crate::OP_TYPE_STRUCT {
                let type_id = ty.operands[0];
                self.get_type_from_id(type_id).ok()
            } else {
                None
            }
        })
    }
    pub fn get_entry_points(&self) -> impl Iterator<Item = String> {
        self.instructions.iter().filter_map(|i| {
            if i.opcode != crate::OP_ENTRY_POINT {
                return None;
            }

            let _execution_model = i.operands[0];
            let _entry_point_id = i.operands[1];

            let entry_point_name = Self::parse_string_literal(&i.operands[1..]);

            Some(entry_point_name)
        })
    }
}

#[cfg(test)]
mod tests {
    // use crate::module::Module;
    // use crate::module::ShaderIoInfo;

    #[test]
    fn test1() {
        // let shader_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        //     .join("..")
        //     .join("files")
        //     .join("compiled-shaders")
        //     .join("shader.vert.spv");
        // println!("{}", env::current_dir().unwrap().display());
        // let m = Module::from_file(&shader_path)
        //     .expect(&format!("failed to load {}", shader_path.display()));

        // let info: Vec<ShaderIoInfo> = m.get_inputs().collect();
        // println!("{:?}", info);
        assert_eq!(1, 1);
    }
}
