/// A nul-terminated Windows UTF-16LE string, used for passing strings to the
/// Windows API.
pub struct WideString(Vec<u16>);

impl WideString {
    /// Creates a new wide string from a pointer and length, nul-terminating the
    /// string if necessary.
    ///
    /// # Safety
    /// - The pointer must be valid for `len` elements.
    pub unsafe fn from_ptr(ptr: *const u16, len: usize) -> Self {
        unsafe { Self::from_iter(std::slice::from_raw_parts(ptr, len).iter().copied()) }
    }

    /// Converts the wide string to a Rust string, lossily.
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(self.as_slice())
    }

    /// Returns a pointer to the underlying wide string, which is
    /// nul-terminated.
    pub fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }

    /// Returns a slice of the underlying wide string, excluding the nul
    /// terminator.
    pub fn as_slice(&self) -> &[u16] {
        self.0.get(..self.0.len() - 1).unwrap_or(&[])
    }

    /// Returns a slice of the underlying wide string, including the nul
    /// terminator.
    pub fn as_slice_with_nul(&self) -> &[u16] {
        &self.0
    }

    /// Returns a slice of the underlying wide string as bytes, excluding the
    /// nul terminator.
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        u16tou8(self.as_slice_with_nul())
    }
}

impl From<&str> for WideString {
    /// Creates a new wide string from a Rust string. Cuts the string at the
    /// first nul terminator if present.
    fn from(s: &str) -> Self {
        Self::from_iter(s.encode_utf16())
    }
}

impl FromIterator<u16> for WideString {
    /// Creates a new wide string from an iterator of u16 values, stopping at
    /// the first nul terminator if present.
    fn from_iter<T: IntoIterator<Item = u16>>(iter: T) -> Self {
        let mut wide: Vec<u16> = iter.into_iter().take_while(|&c| c != 0).collect();
        wide.push(0); // nul terminator
        Self(wide)
    }
}

impl FromIterator<u8> for WideString {
    /// Creates a new wide string from an iterator of u8 values, interpreting
    /// them as little-endian u16 values, stopping at the first nul terminator
    /// if present.
    fn from_iter<T: IntoIterator<Item = u8>>(iter: T) -> Self {
        let mut iter = iter.into_iter();
        Self::from_iter(std::iter::from_fn(move || {
            let msb = iter.next()?;
            let lsb = iter.next()?;
            Some(u16::from_le_bytes([msb, lsb]))
        }))
    }
}

/// Converts a slice of u16 to a slice of u8, without copying.
fn u16tou8(slice: &[u16]) -> &[u8] {
    // SAFETY: u16 has larger alignment than u8, so the pointer is valid for
    // u8. We use `size_of_val` to get the correct length in bytes, and the slice is
    // valid for the lifetime of the input slice.
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}
