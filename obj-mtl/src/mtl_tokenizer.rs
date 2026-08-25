use crate::{Channel, Error, Result};

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[allow(unused)]
pub(crate) const IMFCHAN_R: u16 = 1 << 0;
#[allow(unused)]
pub(crate) const IMFCHAN_G: u16 = 1 << 1;
#[allow(unused)]
pub(crate) const IMFCHAN_B: u16 = 1 << 2;
#[allow(unused)]
pub(crate) const IMFCHAN_M: u16 = 1 << 3;
#[allow(unused)]
pub(crate) const IMFCHAN_L: u16 = 1 << 4;
#[allow(unused)]
pub(crate) const IMFCHAN_Z: u16 = 1 << 5;

#[derive(Debug)]
pub(crate) struct Mm {
    pub(crate) base: f32,
    pub(crate) gain: f32,
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) struct MtlOptions {
    pub(crate) bm: Option<f32>,
    pub(crate) blendu: Option<bool>,
    pub(crate) blendv: Option<bool>,
    pub(crate) cc: Option<bool>,
    pub(crate) clamp: Option<bool>,
    pub(crate) imfchan: Option<Channel>,
    pub(crate) mm: Option<Mm>,
    pub(crate) o: Option<[f32; 3]>,
    pub(crate) s: Option<[f32; 3]>,
    pub(crate) t: Option<[f32; 3]>,
    pub(crate) texres: Option<u32>,
    pub(crate) boost: Option<f32>,
}

impl Default for MtlOptions {
    fn default() -> Self {
        Self {
            bm: None,
            blendu: None,
            blendv: None,
            cc: None,
            clamp: None,
            imfchan: None,
            mm: None,
            o: None,
            s: None,
            t: None,
            texres: None,
            boost: None,
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) enum MtlToken {
    Material(Box<str>),
    Ka {
        r: f32,
        g: f32,
        b: f32,
    },
    Kd {
        r: f32,
        g: f32,
        b: f32,
    },
    Ks {
        r: f32,
        g: f32,
        b: f32,
    },
    MapKa {
        options: MtlOptions,
        file_name: Box<str>,
    },
    MapKd {
        options: MtlOptions,
        file_name: Box<str>,
    },
    MapKs {
        options: MtlOptions,
        file_name: Box<str>,
    },
    Ns(f32),
    MapNs {
        options: MtlOptions,
        file_name: Box<str>,
    },
    Ni(f32),
    Illum(u32),
    Bump {
        options: MtlOptions,
        file_name: Box<str>,
    },
    D(f32),
    Tr(f32),
}

#[allow(unused)]
pub(crate) struct MtlTokenizer {
    line: String,
    reader: BufReader<File>,
}

#[allow(unused)]
impl MtlTokenizer {
    pub(crate) fn from_file(file: File) -> Self {
        let reader = BufReader::new(file);

        Self {
            line: String::with_capacity(64),
            reader,
        }
    }
    pub(crate) fn from_path(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;

        Ok(Self::from_file(file))
    }
    fn skip_ws(bytes: &[u8], i: &mut usize) {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn next_token_as_str<'a>(s: &'a str, i: &mut usize) -> Option<(usize, &'a str)> {
        let bytes = s.as_bytes();
        Self::skip_ws(bytes, i);
        if *i >= bytes.len() {
            return None;
        }

        let start = *i;
        while *i < bytes.len() && !bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }

        Some((start, &s[start..*i]))
    }
    fn parse_v3(rest: &str, i: &mut usize) -> Result<(f32, f32, f32)> {
        let (_, r) = Self::next_token_as_str(rest, i).ok_or(Error::Parse("No r value"))?;
        let r = r
            .parse::<f32>()
            .map_err(|_| Error::Parse("Invalid r value"))?;

        let (_, g) = Self::next_token_as_str(rest, i).ok_or(Error::Parse("No g value"))?;
        let g = g
            .parse::<f32>()
            .map_err(|_| Error::Parse("Invalid g value"))?;

        let (_, b) = Self::next_token_as_str(rest, i).ok_or(Error::Parse("No b value"))?;
        let b = b
            .parse::<f32>()
            .map_err(|_| Error::Parse("Invalid b value"))?;

        Ok((r, g, b))
    }
    fn parse_map_args(rest: &str) -> Option<(MtlOptions, Box<str>)> {
        let mut i = 0;

        let mut options = MtlOptions::default();

        while let Some((start, token)) = Self::next_token_as_str(rest, &mut i) {
            if !token.starts_with('-') {
                let filename = &rest[start..];
                return Some((options, filename.into()));
            }

            match token {
                "-bm" => {
                    let (_, bm) = Self::next_token_as_str(rest, &mut i)?;
                    let bm = bm.parse().ok()?;

                    options.bm = Some(bm)
                }
                "-blendu" => {
                    let (_, blendu) = Self::next_token_as_str(rest, &mut i)?;

                    match blendu {
                        "on" => options.blendu = Some(true),
                        "off" => options.blendu = Some(false),
                        _ => return None,
                    }
                }
                "-blendv" => {
                    let (_, blendv) = Self::next_token_as_str(rest, &mut i)?;

                    match blendv {
                        "on" => options.blendv = Some(true),
                        "off" => options.blendv = Some(false),
                        _ => return None,
                    }
                }
                "-cc" => {
                    let (_, cc) = Self::next_token_as_str(rest, &mut i)?;

                    match cc {
                        "on" => options.cc = Some(true),
                        "off" => options.cc = Some(false),
                        _ => return None,
                    }
                }
                "-clamp" => {
                    let (_, clamp) = Self::next_token_as_str(rest, &mut i)?;

                    match clamp {
                        "on" => options.clamp = Some(true),
                        "off" => options.clamp = Some(false),
                        _ => return None,
                    }
                }
                "-imfchan" => {
                    let (_, channel) = Self::next_token_as_str(rest, &mut i)?;
                    let channel = Channel::from_str(channel)?;

                    options.imfchan = Some(channel);
                }
                "-mm" => {
                    let (_, base) = Self::next_token_as_str(rest, &mut i)?;
                    let base = base.parse().ok()?;

                    let (_, gain) = Self::next_token_as_str(rest, &mut i)?;
                    let gain = gain.parse().ok()?;

                    options.mm = Some(Mm { base, gain })
                }
                "-o" => {
                    let (u, v, w) = Self::parse_v3(rest, &mut i).ok()?;
                    options.o = Some([u, v, w])
                }
                "-s" => {
                    let (u, v, w) = Self::parse_v3(rest, &mut i).ok()?;
                    options.s = Some([u, v, w]);
                }
                "-t" => {
                    let (u, v, w) = Self::parse_v3(rest, &mut i).ok()?;
                    options.t = Some([u, v, w]);
                }
                "-texres" => {
                    let (_, texres) = Self::next_token_as_str(rest, &mut i)?;
                    let texres = texres.parse().ok()?;

                    options.texres = Some(texres);
                }
                "-boost" => {
                    let (_, boost) = Self::next_token_as_str(rest, &mut i)?;
                    let boost = boost.parse().ok()?;

                    options.boost = Some(boost);
                }
                _ => {
                    let filename = &rest[start..];

                    return Some((options, filename.into()));
                }
            }
        }

        return None;
    }
    pub(crate) fn next_token(&mut self) -> Option<Result<MtlToken>> {
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
        let rest = line.next().unwrap_or("");

        let token = match keyword {
            "newmtl" => MtlToken::Material(rest.into()),
            "Ka" => {
                let mut i = 0;
                let (r, g, b) = match Self::parse_v3(rest, &mut i) {
                    Ok(rgb) => rgb,
                    Err(e) => return Some(Err(e)),
                };

                MtlToken::Ka { r, g, b }
            }
            "Kd" => {
                let mut i = 0;
                let (r, g, b) = match Self::parse_v3(rest, &mut i) {
                    Ok(rgb) => rgb,
                    Err(e) => return Some(Err(e)),
                };

                MtlToken::Kd { r, g, b }
            }
            "Ks" => {
                let mut i = 0;
                let (r, g, b) = match Self::parse_v3(rest, &mut i) {
                    Ok(rgb) => rgb,
                    Err(e) => return Some(Err(e)),
                };

                MtlToken::Ks { r, g, b }
            }
            "map_Kd" => {
                let (options, file_name) = match Self::parse_map_args(rest) {
                    Some(res) => res,
                    None => return Some(Err(Error::Parse("map_Kd parse error"))),
                };

                MtlToken::MapKd { options, file_name }
            }
            "map_Ks" => {
                let (options, file_name) = match Self::parse_map_args(rest) {
                    Some(res) => res,
                    None => return Some(Err(Error::Parse("map_Ks parse error"))),
                };

                MtlToken::MapKs { options, file_name }
            }
            "map_Ns" => {
                let (options, file_name) = match Self::parse_map_args(rest) {
                    Some(res) => res,
                    None => return Some(Err(Error::Parse("map_Ns parse error"))),
                };

                MtlToken::MapNs { options, file_name }
            }
            "Ns" => {
                let ns = match rest.trim().parse::<f32>() {
                    Ok(f) => f,
                    _ => return Some(Err(Error::Parse("Invalid Ns value"))),
                };

                MtlToken::Ns(ns)
            }
            "Ni" => {
                let ni = match rest.trim().parse::<f32>() {
                    Ok(f) => f,
                    _ => return Some(Err(Error::Parse("Invalid Ni value"))),
                };

                MtlToken::Ni(ni)
            }
            "illum" => {
                let illum = match rest.trim().parse::<u32>() {
                    Ok(i) => i,
                    _ => return Some(Err(Error::Parse("Invalid illum value"))),
                };

                MtlToken::Illum(illum)
            }
            "bump" | "map_bump" | "map_Bump" => {
                let (options, file_name) = match Self::parse_map_args(rest) {
                    Some(res) => res,
                    None => return Some(Err(Error::Parse("bump parse error"))),
                };

                MtlToken::Bump { options, file_name }
            }
            "d" => {
                let d = match rest.trim().parse::<f32>() {
                    Ok(f) => f,
                    _ => return Some(Err(Error::Parse("Invalid d value"))),
                };

                MtlToken::D(d)
            }
            "tr" => {
                let tr = match rest.trim().parse::<f32>() {
                    Ok(f) => f,
                    _ => return Some(Err(Error::Parse("Invalid tr value"))),
                };

                MtlToken::Tr(tr)
            }
            // "disp" => todo!(),
            // "refl" => todo!(),
            // "decal" => todo!(),
            // "d" => todo!(),
            // "map_d" => todo!(),
            // "Pr" => todo!(),
            // "map_Pr" => todo!(),
            // "Pm" => todo!(),
            // "map_Pm" => todo!(),
            // "Pc" => todo!(),
            // "Pcr" => todo!(),
            // "Ke" => todo!(),
            // "map_Ke" => todo!(),
            // "aniso" => todo!(),
            // "anisor" => todo!(),
            // "norm" => todo!(),
            // "map_RMA" => todo!(),
            // "map_ORM" => todo!(),
            _ => return self.next_token(),
        };

        Some(Ok(token))
    }
}
