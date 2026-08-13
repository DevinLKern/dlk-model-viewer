#[derive(Debug)]
pub enum Error {
    LoadingError(ash::LoadingError),
    NulError(std::ffi::NulError),
    CouldNotFindLayer(std::ffi::CString),
    CouldNotFindExtension(std::ffi::CString),
    VkError(ash::vk::Result),
    NoViablePhysicalDevices,
    IoError(std::io::Error),
    TooManyDescriptorSets,
    CouldNotDetermineEntryPointName,
    CouldNotDetermineFormat,
    CouldNotGetSurfaceFormats(ash::vk::Result),
    NoSurfaceFomratsSupported,
    CouldNotFindMemoryTypeIndex(ash::vk::MemoryPropertyFlags),
    InvalidBufferType,
    WinitHandleError(winit::raw_window_handle::HandleError),
    ImageFeaturesNotSupported,
    CouldNotFindViableMemoryIndex,
    NotImplemented,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadingError(e) => write!(f, "Failed to load Vulkan: {}", e),
            Self::NulError(e) => write!(f, "Encountered null byte where not allowed: {}", e),
            Self::CouldNotFindLayer(l) => write!(f, "Could not find required layer: {:?}", l),
            Self::CouldNotFindExtension(e) => {
                write!(f, "Could not find required extension: {:?}", e)
            }
            Self::VkError(r) => write!(f, "Vk error: {:?}", r),
            Self::NoViablePhysicalDevices => write!(f, "No viable physical devices found"),
            Self::IoError(e) => write!(f, "I/O error: {}", e),
            Self::TooManyDescriptorSets => write!(f, "Too many descriptor sets allocated"),
            Self::CouldNotDetermineFormat => write!(f, "Could not determine format"),
            Self::CouldNotGetSurfaceFormats(r) => {
                write!(f, "Failed to get surface formats: {:?}", r)
            }
            Self::NoSurfaceFomratsSupported => write!(f, "No surface formats supported"),
            Self::CouldNotFindMemoryTypeIndex(flags) => {
                write!(f, "Could not find memory type index with flags {:?}", flags)
            }
            Self::InvalidBufferType => write!(f, "Invalid buffer type"),
            Self::ImageFeaturesNotSupported => write!(f, "ImageFeaturesNotSupported"),
            Self::CouldNotFindViableMemoryIndex => write!(f, "CouldNotFindViableMemoryIndex"),
            _ => write!(f, "Not implemented"),
        }
    }
}

impl From<winit::raw_window_handle::HandleError> for Error {
    fn from(value: winit::raw_window_handle::HandleError) -> Self {
        Self::WinitHandleError(value)
    }
}

impl From<ash::LoadingError> for Error {
    fn from(value: ash::LoadingError) -> Self {
        Self::LoadingError(value)
    }
}

impl From<std::ffi::NulError> for Error {
    fn from(value: std::ffi::NulError) -> Self {
        Self::NulError(value)
    }
}

impl From<ash::vk::Result> for Error {
    #[inline]
    fn from(value: ash::vk::Result) -> Self {
        Self::VkError(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
