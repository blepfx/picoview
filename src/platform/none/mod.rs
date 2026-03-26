pub unsafe fn open_window(
    _: crate::WindowBuilder,
    _: super::OpenMode,
) -> Result<crate::WindowWaker, crate::WindowError> {
    Err(crate::WindowError::Platform(
        "unsupported platform".to_string(),
    ))
}
