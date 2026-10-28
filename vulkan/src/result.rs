#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to load Vulkan: {0}")]
    LoadingError(#[from] ash::LoadingError),

    #[error("Invalid null byte: {0}")]
    NulError(#[from] std::ffi::NulError),

    #[error("Could not find required layer: {0:?}")]
    CouldNotFindLayer(std::ffi::CString),

    #[error("Could not find required extension: {0:?}")]
    CouldNotFindExtension(std::ffi::CString),

    #[error("Vulkan error: {0:?}")]
    VkError(#[from] ash::vk::Result),

    #[error("No viable physical devices found")]
    NoViablePhysicalDevices,

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Too many descriptor sets allocated")]
    TooManyDescriptorSets,

    #[error("Could not determine entry point name")]
    CouldNotDetermineEntryPointName,

    #[error("Could not determine format")]
    CouldNotDetermineFormat,

    #[error("Failed to get surface formats: {0:?}")]
    CouldNotGetSurfaceFormats(ash::vk::Result),

    #[error("No surface formats supported")]
    NoSurfaceFomratsSupported,

    #[error("Could not find memory type index with flags {0:?}")]
    CouldNotFindMemoryTypeIndex(ash::vk::MemoryPropertyFlags),

    #[error("Invalid buffer type")]
    InvalidBufferType,

    #[error("Failed to get window handle: {0}")]
    WinitHandleError(#[from] winit::raw_window_handle::HandleError),

    #[error("Image features not supported")]
    ImageFeaturesNotSupported,

    #[error("Could not find a viable memory index")]
    CouldNotFindViableMemoryIndex,

    #[error("Requested range is outside allocation bounds")]
    OutsideAllocationRange,

    #[error("Feature not implemented")]
    NotImplemented,
}

pub type Result<T> = std::result::Result<T, Error>;
