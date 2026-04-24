#[derive(Debug)]
pub enum Error {
    VulkanError(vulkan::result::Error),
    ExpectedUniformBufferView,
    NotAdded,
    BufferCapacityExceeded {
        buffer: &'static str,
        requested_end: u64,
        capacity: u64,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VulkanError(e) => write!(f, "VulkanError({})", e),
            Self::BufferCapacityExceeded {
                buffer,
                requested_end,
                capacity,
            } => write!(
                f,
                "BufferCapacityExceeded(buffer={buffer}, requested_end={requested_end}, capacity={capacity})"
            ),
            _ => write!(f, "Error type not added yet"),
        }
    }
}

impl From<ash::vk::Result> for Error {
    #[inline]
    fn from(value: ash::vk::Result) -> Self {
        Self::VulkanError(value.into())
    }
}

impl From<vulkan::result::Error> for Error {
    fn from(value: vulkan::result::Error) -> Self {
        Self::VulkanError(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
