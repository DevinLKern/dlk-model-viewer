#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Vulkan operation failed")]
    VulkanError(#[from] vulkan::result::Error),

    #[error("expected a uniform buffer view")]
    ExpectedUniformBufferView,

    #[error("resource missing: {0}")]
    ResourceMissing(&'static str),

    #[error("buffer capacity exceeded")]
    BufferCapacityExceeded,

    #[error("failed to convert boxed value to slice")]
    BoxToSliceError,

    #[error("I/O operation failed")]
    IoError(#[from] std::io::Error),
}

impl From<ash::vk::Result> for Error {
    fn from(value: ash::vk::Result) -> Self {
        Self::VulkanError(value.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
