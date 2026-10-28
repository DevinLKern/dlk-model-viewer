mod obj_tokenizer;
pub(crate) use obj_tokenizer::*;

mod mtl_tokenizer;
pub(crate) use mtl_tokenizer::*;

mod obj;
pub use obj::*;

mod mtl;
pub use mtl::*;

#[allow(unused)]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error: {0}")]
    Parse(&'static str),
}

#[allow(unused)]
pub type Result<T> = std::result::Result<T, Error>;
