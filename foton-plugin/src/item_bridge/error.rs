use jni::errors as bridge_jni_errors;
use thiserror::Error;

/// Errors crossing this bridge never stand for an empty item.
#[derive(Debug, Error)]
pub(crate) enum ItemBridgeError {
    #[error("item registry is not initialized")]
    RegistryNotReady,
    #[error("native item state cannot be materialized: {0}")]
    NativeState(String),
    #[error("item bridge JNI construction failed: {0}")]
    Jni(#[from] bridge_jni_errors::Error),
    #[error("item metadata exceeds the bridge transport limit")]
    TransportLimit,
    #[error("invalid item edit: {0}")]
    InvalidEdit(&'static str),
    #[error("unsupported item capability: {0}")]
    Unsupported(&'static str),
    #[error("item snapshot limit reached")]
    Capacity,
    #[error("item snapshot epoch is closed")]
    Closed,
    #[error("item snapshot lease is stale or unknown")]
    StaleLease,
    #[error("item snapshot nesting exceeds {0} owned levels")]
    Depth(usize),
    #[error("item snapshot nesting exceeds {0} owned item templates")]
    TemplateDepth(usize),
}

impl ItemBridgeError {
    pub(crate) fn native_state(self) -> Self {
        Self::NativeState(self.to_string())
    }
    pub(crate) fn throw_java(&self, env: &mut jni::JNIEnv<'_>) {
        if !matches!(env.exception_check(), Ok(false)) {
            return;
        }
        let class = match self {
            Self::InvalidEdit(_)
            | Self::Depth(_)
            | Self::TemplateDepth(_)
            | Self::TransportLimit => "java/lang/IllegalArgumentException",
            Self::Unsupported(_) => "java/lang/UnsupportedOperationException",
            _ => "java/lang/IllegalStateException",
        };
        let _ = env.throw_new(class, self.to_string());
    }
}
