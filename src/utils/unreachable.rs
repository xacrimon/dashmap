/// An unreachable function that provides a nice panic for debugging in debug mode
/// and is undefined behaviour in release mode.
pub unsafe fn unreachable() -> ! {
    #[cfg(debug_assertions)]
    unreachable!();

    #[cfg(not(debug_assertions))]
    std::hint::unreachable_unchecked();
}
