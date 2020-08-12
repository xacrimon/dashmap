/// An unreachable function that provides a nice panic for debugging in debug mode
/// and is undefined behaviour in release mode.
pub unsafe fn unreachable() -> ! {
    #[cfg(debug_assertions)]
    unreachable!();

    #[cfg(not(debug_assertions))]
    std::hint::unreachable_unchecked();
}

pub trait UnwrapUnchecked {
    type Value;

    unsafe fn unwrap_unchecked(self) -> Self::Value;
}

impl<T> UnwrapUnchecked for Option<T> {
    type Value = T;

    unsafe fn unwrap_unchecked(self) -> Self::Value {
        if let Some(value) = self {
            value
        } else {
            unreachable()
        }
    }
}

impl<T, E> UnwrapUnchecked for Result<T, E> {
    type Value = T;

    unsafe fn unwrap_unchecked(self) -> Self::Value {
        if let Ok(value) = self {
            value
        } else {
            unreachable()
        }
    }
}
