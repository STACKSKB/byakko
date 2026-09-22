//! Native primary-screen color sampling for host lighting. No frame is saved.
//! One synchronous call reads a sparse 16 × 9 grid and returns its mean RGB.
//! On X11, a server disconnect may invoke Xlib's process-default error handler;
//! callers should stop sampling before the display session ends.

const COLS: i32 = 16;
const ROWS: i32 = 9;

fn mean_rgb(samples: impl IntoIterator<Item = [u8; 3]>) -> Result<[u8; 3], String> {
    let mut totals = [0u64; 3];
    let mut count = 0u64;
    for sample in samples {
        for channel in 0..3 {
            totals[channel] += u64::from(sample[channel]);
        }
        count += 1;
    }
    if count == 0 {
        return Err("No screen pixels were sampled".into());
    }
    Ok(totals.map(|sum| (sum / count) as u8))
}

#[cfg(any(target_os = "linux", test))]
fn grid_coordinate(index: i32, divisions: i32, extent: i32) -> i32 {
    (((index as i64 * 2 + 1) * extent as i64) / (divisions as i64 * 2)) as i32
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{COLS, ROWS, mean_rgb};
    use std::{ffi::c_void, io, ptr::null_mut};

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDC(window: *mut c_void) -> *mut c_void;
        fn ReleaseDC(window: *mut c_void, dc: *mut c_void) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> *mut c_void;
        fn CloseDesktop(desktop: *mut c_void) -> i32;
        fn GetUserObjectInformationW(
            object: *mut c_void,
            index: i32,
            info: *mut c_void,
            length: u32,
            needed: *mut u32,
        ) -> i32;
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn DeleteDC(dc: *mut c_void) -> i32;
        fn CreateDIBSection(
            dc: *mut c_void,
            info: *const BitmapInfo,
            usage: u32,
            bits: *mut *mut c_void,
            section: *mut c_void,
            offset: u32,
        ) -> *mut c_void;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn SetStretchBltMode(dc: *mut c_void, mode: i32) -> i32;
        fn StretchBlt(
            dest: *mut c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            src: *mut c_void,
            src_x: i32,
            src_y: i32,
            src_width: i32,
            src_height: i32,
            operation: u32,
        ) -> i32;
        fn GdiFlush() -> i32;
    }

    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bits_per_pixel: u16,
        compression: u32,
        image_size: u32,
        x_pixels_per_meter: i32,
        y_pixels_per_meter: i32,
        colors_used: u32,
        colors_important: u32,
    }
    #[repr(C)]
    struct BitmapInfo {
        header: BitmapInfoHeader,
        colors: [u32; 1],
    }

    struct Desktop(*mut c_void);
    impl Drop for Desktop {
        fn drop(&mut self) {
            unsafe { CloseDesktop(self.0) };
        }
    }
    struct ScreenDc(*mut c_void);
    impl Drop for ScreenDc {
        fn drop(&mut self) {
            unsafe { ReleaseDC(null_mut(), self.0) };
        }
    }
    struct SmallBitmap {
        dc: *mut c_void,
        bitmap: *mut c_void,
        previous: *mut c_void,
    }
    impl Drop for SmallBitmap {
        fn drop(&mut self) {
            unsafe {
                if !self.previous.is_null() {
                    SelectObject(self.dc, self.previous);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                DeleteDC(self.dc);
            }
        }
    }

    fn interactive_desktop() -> Result<(), String> {
        // An input desktop other than Default is usually the lock or secure
        // desktop. Refuse to represent that as an all-black user screen.
        let handle = unsafe { OpenInputDesktop(0, 0, 0x0001) }; // DESKTOP_READOBJECTS
        if handle.is_null() {
            return Err(format!(
                "Cannot access interactive desktop: {}",
                io::Error::last_os_error()
            ));
        }
        let desktop = Desktop(handle);
        let mut name = [0u16; 128];
        let mut needed = 0u32;
        let ok = unsafe {
            GetUserObjectInformationW(
                desktop.0,
                2, // UOI_NAME
                name.as_mut_ptr().cast(),
                (name.len() * 2) as u32,
                &mut needed,
            )
        };
        if ok == 0 {
            return Err(format!(
                "Cannot identify interactive desktop: {}",
                io::Error::last_os_error()
            ));
        }
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        if String::from_utf16_lossy(&name[..end]) != "Default" {
            return Err("Interactive desktop is locked or unavailable".into());
        }
        Ok(())
    }

    pub struct ScreenSampler;
    impl ScreenSampler {
        pub fn new() -> Result<Self, String> {
            interactive_desktop()?;
            let (width, height) = unsafe { (GetSystemMetrics(0), GetSystemMetrics(1)) };
            if width <= 0 || height <= 0 {
                return Err("Primary display is unavailable".into());
            }
            Ok(Self)
        }

        pub fn sample(&mut self) -> Result<[u8; 3], String> {
            interactive_desktop()?;
            let (width, height) = unsafe { (GetSystemMetrics(0), GetSystemMetrics(1)) };
            if width <= 0 || height <= 0 {
                return Err("Primary display is unavailable".into());
            }
            let handle = unsafe { GetDC(null_mut()) };
            if handle.is_null() {
                return Err(format!(
                    "Screen DC unavailable: {}",
                    io::Error::last_os_error()
                ));
            }
            let dc = ScreenDc(handle);
            let memory_dc = unsafe { CreateCompatibleDC(dc.0) };
            if memory_dc.is_null() {
                return Err(format!(
                    "Memory DC unavailable: {}",
                    io::Error::last_os_error()
                ));
            }
            let mut small = SmallBitmap {
                dc: memory_dc,
                bitmap: null_mut(),
                previous: null_mut(),
            };
            let info = BitmapInfo {
                header: BitmapInfoHeader {
                    size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                    width: COLS,
                    height: -ROWS, // top-down 32-bit BGRA
                    planes: 1,
                    bits_per_pixel: 32,
                    compression: 0, // BI_RGB
                    image_size: 0,
                    x_pixels_per_meter: 0,
                    y_pixels_per_meter: 0,
                    colors_used: 0,
                    colors_important: 0,
                },
                colors: [0],
            };
            let mut bits = null_mut();
            small.bitmap = unsafe { CreateDIBSection(dc.0, &info, 0, &mut bits, null_mut(), 0) };
            if small.bitmap.is_null() || bits.is_null() {
                return Err(format!(
                    "Screen DIB unavailable: {}",
                    io::Error::last_os_error()
                ));
            }
            small.previous = unsafe { SelectObject(small.dc, small.bitmap) };
            if small.previous.is_null() || small.previous as isize == -1 {
                small.previous = null_mut();
                return Err(format!(
                    "Could not select screen DIB: {}",
                    io::Error::last_os_error()
                ));
            }
            if unsafe { SetStretchBltMode(small.dc, 4) } == 0 {
                // HALFTONE
                return Err(format!(
                    "Screen stretch mode unavailable: {}",
                    io::Error::last_os_error()
                ));
            }
            if unsafe {
                StretchBlt(
                    small.dc,
                    0,
                    0,
                    COLS,
                    ROWS,
                    dc.0,
                    0,
                    0,
                    width,
                    height,
                    0x00cc_0020,
                )
            } == 0
            {
                return Err(format!(
                    "Screen transfer failed: {}",
                    io::Error::last_os_error()
                ));
            }
            if unsafe { GdiFlush() } == 0 {
                return Err(format!(
                    "Screen transfer did not flush: {}",
                    io::Error::last_os_error()
                ));
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(bits.cast::<u8>(), (COLS * ROWS * 4) as usize)
            };
            mean_rgb(
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|pixel| [pixel[2], pixel[1], pixel[0]]),
            )
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{COLS, ROWS, grid_coordinate, mean_rgb};
    use std::ffi::{CStr, CString, c_void};

    type Display = c_void;
    type Drawable = libc::c_ulong;
    type OpenDisplay = unsafe extern "C" fn(*const libc::c_char) -> *mut Display;
    type CloseDisplay = unsafe extern "C" fn(*mut Display) -> libc::c_int;
    type DefaultScreen = unsafe extern "C" fn(*mut Display) -> libc::c_int;
    type RootWindow = unsafe extern "C" fn(*mut Display, libc::c_int) -> Drawable;
    type DisplayExtent = unsafe extern "C" fn(*mut Display, libc::c_int) -> libc::c_int;
    type GetImage = unsafe extern "C" fn(
        *mut Display,
        Drawable,
        i32,
        i32,
        u32,
        u32,
        libc::c_ulong,
        i32,
    ) -> *mut XImage;

    #[repr(C)]
    struct XImage {
        width: i32,
        height: i32,
        xoffset: i32,
        format: i32,
        data: *mut libc::c_char,
        byte_order: i32,
        bitmap_unit: i32,
        bitmap_bit_order: i32,
        bitmap_pad: i32,
        depth: i32,
        bytes_per_line: i32,
        bits_per_pixel: i32,
        red_mask: libc::c_ulong,
        green_mask: libc::c_ulong,
        blue_mask: libc::c_ulong,
        obdata: *mut c_void,
        funcs: XImageFuncs,
    }
    #[repr(C)]
    struct XImageFuncs {
        create_image: *mut c_void,
        destroy_image: unsafe extern "C" fn(*mut XImage) -> i32,
        get_pixel: unsafe extern "C" fn(*mut XImage, i32, i32) -> libc::c_ulong,
        put_pixel: *mut c_void,
        sub_image: *mut c_void,
        add_pixel: *mut c_void,
    }
    struct Image(*mut XImage);
    impl Drop for Image {
        fn drop(&mut self) {
            unsafe { ((*self.0).funcs.destroy_image)(self.0) };
        }
    }

    struct Xlib {
        handle: *mut c_void,
        open: OpenDisplay,
        close: CloseDisplay,
        default_screen: DefaultScreen,
        root: RootWindow,
        width: DisplayExtent,
        height: DisplayExtent,
        image: GetImage,
    }
    impl Drop for Xlib {
        fn drop(&mut self) {
            unsafe { libc::dlclose(self.handle) };
        }
    }
    impl Xlib {
        fn load() -> Result<Self, String> {
            let handle =
                unsafe { libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
            if handle.is_null() {
                let error = unsafe { CStr::from_ptr(libc::dlerror()) }.to_string_lossy();
                return Err(format!("X11 library unavailable: {error}"));
            }
            macro_rules! sym {
                ($name:literal, $ty:ty) => {{
                    let pointer =
                        unsafe { libc::dlsym(handle, concat!($name, "\0").as_ptr().cast()) };
                    if pointer.is_null() {
                        unsafe { libc::dlclose(handle) };
                        return Err(format!("X11 symbol {} unavailable", $name));
                    }
                    unsafe { std::mem::transmute::<*mut c_void, $ty>(pointer) }
                }};
            }
            Ok(Self {
                handle,
                open: sym!("XOpenDisplay", OpenDisplay),
                close: sym!("XCloseDisplay", CloseDisplay),
                default_screen: sym!("XDefaultScreen", DefaultScreen),
                root: sym!("XRootWindow", RootWindow),
                width: sym!("XDisplayWidth", DisplayExtent),
                height: sym!("XDisplayHeight", DisplayExtent),
                image: sym!("XGetImage", GetImage),
            })
        }
    }

    pub struct ScreenSampler {
        xlib: Xlib,
        display: *mut Display,
    }
    impl Drop for ScreenSampler {
        fn drop(&mut self) {
            unsafe { (self.xlib.close)(self.display) };
        }
    }
    impl ScreenSampler {
        pub fn new() -> Result<Self, String> {
            if std::env::var("XDG_SESSION_TYPE")
                .is_ok_and(|value| value.eq_ignore_ascii_case("wayland"))
                || std::env::var("WAYLAND_DISPLAY").is_ok_and(|value| !value.is_empty())
            {
                return Err("Wayland screen sampling is unsupported; XWayland root colors may be incomplete".into());
            }
            let display_name = std::env::var("DISPLAY").map_err(|_| {
                "DISPLAY is unset; pure Wayland screen sampling is unsupported".to_string()
            })?;
            let name =
                CString::new(display_name).map_err(|_| "Invalid DISPLAY name".to_string())?;
            let xlib = Xlib::load()?;
            let display = unsafe { (xlib.open)(name.as_ptr()) };
            if display.is_null() {
                return Err("Cannot connect to the X11 display".into());
            }
            Ok(Self { xlib, display })
        }

        pub fn sample(&mut self) -> Result<[u8; 3], String> {
            let screen = unsafe { (self.xlib.default_screen)(self.display) };
            let (width, height) = unsafe {
                (
                    (self.xlib.width)(self.display, screen),
                    (self.xlib.height)(self.display, screen),
                )
            };
            if width <= 0 || height <= 0 {
                return Err("X11 primary screen is unavailable".into());
            }
            let root = unsafe { (self.xlib.root)(self.display, screen) };
            let mut colors = Vec::with_capacity((COLS * ROWS) as usize);
            for row in 0..ROWS {
                // Fetch one scanline rather than a full screen image. Nine
                // short X11 requests keep memory and transfer cost bounded.
                let image = unsafe {
                    (self.xlib.image)(
                        self.display,
                        root,
                        0,
                        grid_coordinate(row, ROWS, height),
                        width as u32,
                        1,
                        !0,
                        2,
                    )
                }; // ZPixmap
                if image.is_null() {
                    return Err("X11 screen capture failed or desktop is unavailable".into());
                }
                let image = Image(image);
                let masks = unsafe {
                    [
                        (*image.0).red_mask,
                        (*image.0).green_mask,
                        (*image.0).blue_mask,
                    ]
                };
                if masks.contains(&0) {
                    return Err("X11 visual has unsupported color masks".into());
                }
                for col in 0..COLS {
                    let pixel = unsafe {
                        ((*image.0).funcs.get_pixel)(image.0, grid_coordinate(col, COLS, width), 0)
                    };
                    colors.push(masks.map(|mask| {
                        let shift = mask.trailing_zeros();
                        let max = mask >> shift;
                        (((pixel & mask) >> shift) * 255 / max) as u8
                    }));
                }
            }
            mean_rgb(colors)
        }
    }
}

pub use platform::ScreenSampler;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn averages_rgb_without_alpha_or_spatial_bias() {
        assert_eq!(
            mean_rgb([[0, 100, 200], [100, 0, 0]]).unwrap(),
            [50, 50, 100]
        );
        assert!(mean_rgb([]).is_err());
        assert_eq!(grid_coordinate(0, 16, 160), 5);
        assert_eq!(grid_coordinate(15, 16, 160), 155);
    }
}
