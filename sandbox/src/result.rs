#[derive(Debug)]
pub enum Error {
    IoError(std::io::Error),
    WinitExternalError(winit::error::ExternalError),
    WinitEventLoopError(winit::error::EventLoopError),
    WinitHandleError(winit::raw_window_handle::HandleError),
    VulkanError(vulkan::result::Error),
    ImageError(image::ImageError),
    ObjMtlError(obj_mtl::Error),
    RendererError(renderer::Error),
    YamlEmitError(yaml_rust2::EmitError),
    YamlScanError(yaml_rust2::ScanError),
    ConfigFileInvalid(&'static str),
    MultipleMaterialsPerShape,
    InvalidMaterialIndex,
    CouldNotFindFile,
    WindowIdInvalid,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IoError: {}", e),
            Self::WinitExternalError(e) => write!(f, "ExternalError({e})"),
            Self::WinitEventLoopError(e) => write!(f, "EventLoopError({e})"),
            Self::WinitHandleError(e) => write!(f, "HandleError({e})"),
            Self::VulkanError(e) => write!(f, "VulkanError({e})"),
            Self::ImageError(e) => write!(f, "ImageError({e})"),
            Self::RendererError(e) => write!(f, "RendererError({e})"),
            Self::WindowIdInvalid => write!(f, "WindowIdInvalid"),
            Self::ObjMtlError(e) => write!(f, "ObjMtlError({e})"),
            Self::YamlEmitError(e) => write!(f, "YamlEmitError({e})"),
            Self::YamlScanError(e) => write!(f, "YamlScanError({e})"),
            Self::ConfigFileInvalid(e) => write!(f, "ConfigFileInvalid({e})"),
            Self::MultipleMaterialsPerShape => write!(f, "MultipleMaterialsPerShape"),
            Self::InvalidMaterialIndex => write!(f, "InvalidMaterialIndex"),
            Self::CouldNotFindFile => write!(f, "CouldNotFindFile"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IoError(value)
    }
}

impl From<winit::error::EventLoopError> for Error {
    fn from(value: winit::error::EventLoopError) -> Self {
        Error::WinitEventLoopError(value)
    }
}

impl From<winit::raw_window_handle::HandleError> for Error {
    fn from(value: winit::raw_window_handle::HandleError) -> Self {
        Error::WinitHandleError(value)
    }
}

impl From<vulkan::result::Error> for Error {
    fn from(value: vulkan::result::Error) -> Self {
        Error::VulkanError(value)
    }
}

impl From<image::ImageError> for Error {
    fn from(value: image::ImageError) -> Self {
        Error::ImageError(value)
    }
}

impl From<obj_mtl::Error> for Error {
    fn from(value: obj_mtl::Error) -> Self {
        Error::ObjMtlError(value)
    }
}

impl From<renderer::Error> for Error {
    fn from(value: renderer::Error) -> Self {
        match value {
            renderer::Error::VulkanError(e) => Error::VulkanError(e),
            e => Error::RendererError(e),
        }
    }
}

impl From<winit::error::ExternalError> for Error {
    fn from(value: winit::error::ExternalError) -> Self {
        Error::WinitExternalError(value)
    }
}

impl From<yaml_rust2::EmitError> for Error {
    fn from(value: yaml_rust2::EmitError) -> Self {
        Error::YamlEmitError(value)
    }
}

impl From<yaml_rust2::ScanError> for Error {
    fn from(value: yaml_rust2::ScanError) -> Self {
        Error::YamlScanError(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
