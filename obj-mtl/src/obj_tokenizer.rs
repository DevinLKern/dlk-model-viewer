use crate::{Error, Float, Result};

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Copy, Clone)]
#[allow(unused)]
pub struct VtnIndexRaw {
    pub v: i64,
    pub vt: Option<i64>,
    pub vn: Option<i64>,
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) enum ObjToken {
    MtlFile(Box<str>),
    UseMaterial(Box<str>),
    Shading(u32),
    Object(Box<str>),
    Group(Box<str>),
    V {
        x: Float,
        y: Float,
        z: Float,
        w: Option<Float>,
    },
    Vt {
        u: Float,
        v: Float,
        w: Option<Float>,
    },
    Vn {
        x: Float,
        y: Float,
        z: Float,
    },
    Face(Box<[VtnIndexRaw]>),
    Line(Box<[VtnIndexRaw]>),
    Vp {
        u: Float,
        v: Option<Float>,
        w: Option<Float>,
    },
    Curve {
        //
    },
    Curve2 {
        //
    },
    Surf {
        //
    },
}

#[allow(unused)]
pub(crate) struct ObjTokenizer {
    line: String,
    reader: BufReader<File>,
}

#[allow(unused)]
impl ObjTokenizer {
    pub(crate) fn from_file(file: File) -> Self {
        let reader = BufReader::new(file);

        ObjTokenizer {
            line: String::with_capacity(64),
            reader,
        }
    }
    pub(crate) fn from_path(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        Ok(Self::from_file(file))
    }
    /// Returns None on the EOF
    pub(crate) fn next_token(&mut self) -> Option<Result<ObjToken>> {
        self.line.clear();

        match self.reader.read_line(&mut self.line) {
            Ok(l) => {
                if l == 0 {
                    return None;
                }
            }
            Err(e) => return Some(Err(Error::Io(e))),
        };

        let line = self.line.trim();
        if line.is_empty() {
            return self.next_token();
        }

        let mut line = line.splitn(2, char::is_whitespace);
        let keyword = line.next()?;
        let rest = line.next().unwrap_or("").trim();

        let token: ObjToken = match keyword {
            "mtllib" => ObjToken::MtlFile(rest.into()),
            "usemtl" => ObjToken::UseMaterial(rest.into()),
            "o" => ObjToken::Object(rest.into()),
            "g" => ObjToken::Group(rest.into()),
            "v" => {
                let mut rest = rest.split_whitespace();

                let x = rest.next().map(|s| s.parse::<Float>());
                let y = rest.next().map(|s| s.parse::<Float>());
                let z = rest.next().map(|s| s.parse::<Float>());
                let w = rest.next().map(|s| s.parse::<Float>());

                let (x, y, z, w) = match (x, y, z, w) {
                    (Some(Ok(x)), Some(Ok(y)), Some(Ok(z)), Some(Ok(w))) => (x, y, z, Some(w)),
                    (Some(Ok(x)), Some(Ok(y)), Some(Ok(z)), None) => (x, y, z, Some(1.0)),
                    _ => return Some(Err(Error::Parse("V parsing error"))),
                };

                ObjToken::V { x, y, z, w }
            }
            "vn" => {
                let mut rest = rest.split_whitespace();

                let x = rest.next().map(|s| s.parse::<Float>());
                let y = rest.next().map(|s| s.parse::<Float>());
                let z = rest.next().map(|s| s.parse::<Float>());

                let (x, y, z) = match (x, y, z) {
                    (Some(Ok(x)), Some(Ok(y)), Some(Ok(z))) => (x, y, z),
                    _ => return Some(Err(Error::Parse("Vn component missing"))),
                };

                ObjToken::Vn { x, y, z }
            }
            "vt" => {
                let mut rest = rest.split_whitespace();

                let u = rest.next().map(|s| s.parse::<Float>());
                let v = rest.next().map(|s| s.parse::<Float>());
                let w = rest.next().map(|s| s.parse::<Float>());

                let (u, v, w) = match (u, v, w) {
                    (Some(Ok(u)), Some(Ok(v)), Some(Ok(w))) => (u, v, Some(w)),
                    (Some(Ok(u)), Some(Ok(v)), None) => (u, v, None),
                    _ => return Some(Err(Error::Parse("Vt parsing error"))),
                };

                ObjToken::Vt { u, v, w }
            }
            "f" => {
                let rest = rest.split_whitespace();

                let mut vertices = Vec::with_capacity(4);

                // TODO: this code works, but it's kinda gross. Maybe refactor it sometime.
                for part in rest {
                    let mut components = part.split('/');
                    let v = match components.next() {
                        Some(v) => v.parse::<i64>(),
                        None => return Some(Err(Error::Parse("Vertex component missing"))),
                    };
                    let v = match v {
                        Ok(i) => i,
                        _ => return Some(Err(Error::Parse("Vertex parsing error"))),
                    };

                    let vt = match components.next() {
                        Some(s) => match s {
                            "" => None,
                            _ => Some(s.parse::<i64>()),
                        },
                        None => None,
                    };
                    let vt = match vt {
                        Some(Ok(i)) => Some(i),
                        None => None,
                        _ => return Some(Err(Error::Parse("Vertex parsing error"))),
                    };

                    let vn = match components.next() {
                        Some(s) => match s {
                            "" => None,
                            _ => Some(s.parse::<i64>()),
                        },
                        None => None,
                    };
                    let vn = match vn {
                        Some(Ok(i)) => Some(i),
                        None => None,
                        _ => return Some(Err(Error::Parse("Vertex parsing error"))),
                    };

                    vertices.push(VtnIndexRaw { v, vt, vn });
                }

                ObjToken::Face(vertices.into_boxed_slice())
            }
            "l" => {
                let rest = rest.split_whitespace();

                let mut indices = Vec::<VtnIndexRaw>::with_capacity(4);

                for part in rest {
                    let mut parts = part.splitn(2, "/");
                    let v = match parts.next() {
                        Some(s) => s.parse::<i64>(),
                        _ => return Some(Err(Error::Parse("Line vertex missing"))),
                    };
                    let v = match v {
                        Ok(i) => i,
                        _ => return Some(Err(Error::Parse("Line vertex invalid"))),
                    };

                    let vt = match parts.next() {
                        Some(s) => Some(s.parse::<i64>()),
                        None => None,
                    };
                    let vt = match vt {
                        Some(Ok(i)) => Some(i),
                        None => None,
                        _ => return Some(Err(Error::Parse("Line vt problem"))),
                    };

                    indices.push(VtnIndexRaw { v, vt, vn: None });
                }

                ObjToken::Line(indices.into_boxed_slice())
            }
            "s" => {
                let s = match rest.trim() {
                    "off" => 0,
                    x => match x.parse::<u32>() {
                        Ok(y) => y,
                        _ => return Some(Err(Error::Parse("Shading parsing error"))),
                    },
                };

                ObjToken::Shading(s)
            }
            "vp" => {
                let mut rest = rest.split_whitespace();

                let u = rest.next().map(|s| s.parse::<Float>());
                let v = rest.next().map(|s| s.parse::<Float>());
                let w = rest.next().map(|s| s.parse::<Float>());

                let (u, v, w) = match (u, v, w) {
                    (Some(Ok(u)), Some(Ok(v)), Some(Ok(w))) => (u, Some(v), Some(w)),
                    (Some(Ok(u)), Some(Ok(v)), None) => (u, Some(v), None),
                    (Some(Ok(u)), None, None) => (u, None, None),
                    _ => return Some(Err(Error::Parse("Vp parsing error"))),
                };

                ObjToken::Vp { u, v, w }
            }
            // "p" => {
            //     todo!()
            // }
            // "mg" => {
            //     todo!()
            // }
            // "sp" => {
            //     todo!()
            // }
            // "scrv" => {
            //     todo!()
            // }
            // "con" => {
            //     todo!()
            // }
            // "parm" => {
            //     todo!()
            // }
            // "curv" => {
            //     todo!()
            // }
            // "curv2" => {
            //     todo!()
            // }
            // "cstype" => {
            //     todo!()
            // }
            // "deg" => {
            //     todo!()
            // }
            // "surf" => {
            //     todo!()
            // }
            // "trim" => {
            //     todo!()
            // }
            // "hole" => {
            //     todo!()
            // }
            // "end" => {
            //     todo!()
            // }
            _ => {
                // comment or unsupported feature or invalid format. skip it.
                return self.next_token();
            }
        };

        Some(Ok(token))
    }
}
