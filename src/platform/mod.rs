#[cfg(all(target_os = "linux", feature = "x11"))]
pub mod x11;
#[cfg(all(target_os = "linux", feature = "wayland"))]
pub mod wayland;


mod sealed {
    pub trait Sealed {}
}
impl sealed::Sealed for crate::Runtime {}