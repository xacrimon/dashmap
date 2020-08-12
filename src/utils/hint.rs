/// An unreachable function that provides a nice panic for debugging in debug mode
/// and is undefined behaviour in release mode.
pub unsafe fn unreachable() -> ! {
    #[cfg(debug_assertions)]
    unreachable!();

    #[cfg(not(debug_assertions))]
    std::hint::unreachable_unchecked();
}

pub trait OptionUnchecked {
    type Value;

    unsafe fn unwrap_unchecked(self) -> Self::Value;
}

impl<T> OptionUnchecked for Option<T> {
    type Value = T;

    unsafe fn unwrap_unchecked(self) -> Self::Value {
        if let Some(value) = self {
            value
        } else {
            unreachable()
        }
    }
}
