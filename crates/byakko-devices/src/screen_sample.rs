//! Native display color sampling for host lighting. No frame is saved.
//! Average reads a sparse 16 × 9 grid; point reads one normalized coordinate.
//! On X11, a server disconnect may invoke Xlib's process-default error handler;
//! callers should stop sampling before the display session ends.

const COLS: i32 = 16;
const ROWS: i32 = 9;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySource {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenSampling {
    #[default]
    Average,
    Point {
        x: u16,
        y: u16,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScreenCapture {
    pub display_id: Option<String>,
    pub sampling: ScreenSampling,
}

fn point_coordinate(value: u16, extent: i32) -> Result<i32, String> {
    if value > 1000 {
        return Err("Screen point coordinates must be within 0..=1000".into());
    }
    if extent <= 0 {
        return Err("Selected display is unavailable".into());
    }
    Ok(((u64::from(value) * (extent as u64 - 1)) / 1000) as i32)
}

fn select_display<'a, T>(
    displays: &'a [T],
    id: Option<&str>,
    identify: impl Fn(&T) -> (&str, bool),
) -> Option<&'a T> {
    displays
        .iter()
        .find(|display| {
            let (candidate, primary) = identify(display);
            id.map_or(primary, |id| candidate == id)
        })
        .or_else(|| if id.is_none() { displays.first() } else { None })
}

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
    use super::{
        COLS, DisplaySource, ROWS, ScreenCapture, ScreenSampling, mean_rgb, point_coordinate,
        select_display,
    };
    use std::{ffi::c_void, io, ptr::null_mut};

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDC(window: *mut c_void) -> *mut c_void;
        fn ReleaseDC(window: *mut c_void, dc: *mut c_void) -> i32;
        fn EnumDisplayMonitors(
            dc: *mut c_void,
            clip: *const c_void,
            callback: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut Rect, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfoExW) -> i32;
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
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct MonitorInfoExW {
        size: u32,
        bounds: Rect,
        work: Rect,
        flags: u32,
        device: [u16; 32],
    }
    #[derive(Clone)]
    struct Monitor {
        source: DisplaySource,
        bounds: Rect,
        primary: bool,
    }

    unsafe extern "system" fn collect_monitor(
        monitor: *mut c_void,
        _: *mut c_void,
        _: *mut Rect,
        data: isize,
    ) -> i32 {
        let monitors = unsafe { &mut *(data as *mut Vec<Monitor>) };
        let mut info = MonitorInfoExW {
            size: std::mem::size_of::<MonitorInfoExW>() as u32,
            bounds: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            work: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            flags: 0,
            device: [0; 32],
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return 0;
        }
        let len = info
            .device
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.device.len());
        let id = String::from_utf16_lossy(&info.device[..len]);
        monitors.push(Monitor {
            source: DisplaySource {
                label: id.clone(),
                id,
            },
            bounds: info.bounds,
            primary: info.flags & 1 != 0,
        });
        1
    }
    fn monitors() -> Result<Vec<Monitor>, String> {
        let mut monitors = Vec::new();
        if unsafe {
            EnumDisplayMonitors(
                null_mut(),
                std::ptr::null(),
                collect_monitor,
                (&mut monitors as *mut Vec<Monitor>) as isize,
            )
        } == 0
        {
            return Err(format!(
                "Cannot enumerate displays: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(monitors)
    }
    pub fn displays() -> Result<Vec<DisplaySource>, String> {
        interactive_desktop()?;
        Ok(monitors()?
            .into_iter()
            .map(|monitor| monitor.source)
            .collect())
    }
    fn selected_monitor(id: Option<&str>) -> Result<Monitor, String> {
        let monitors = monitors()?;
        select_display(&monitors, id, |monitor| {
            (&monitor.source.id, monitor.primary)
        })
        .cloned()
        .ok_or_else(|| {
            if id.is_some() {
                "Selected display is no longer available".into()
            } else {
                "Primary display is unavailable".into()
            }
        })
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

    pub struct ScreenSampler {
        capture: ScreenCapture,
    }
    impl ScreenSampler {
        pub fn new() -> Result<Self, String> {
            Self::with_capture(ScreenCapture::default())
        }

        pub fn with_capture(capture: ScreenCapture) -> Result<Self, String> {
            interactive_desktop()?;
            let monitor = selected_monitor(capture.display_id.as_deref())?;
            let (width, height) = (
                monitor.bounds.right - monitor.bounds.left,
                monitor.bounds.bottom - monitor.bounds.top,
            );
            if width <= 0 || height <= 0 {
                return Err("Selected display is unavailable".into());
            }
            if let ScreenSampling::Point { x, y } = capture.sampling {
                point_coordinate(x, width)?;
                point_coordinate(y, height)?;
            }
            Ok(Self { capture })
        }

        pub fn sample(&mut self) -> Result<[u8; 3], String> {
            interactive_desktop()?;
            let monitor = selected_monitor(self.capture.display_id.as_deref())?;
            let (width, height) = (
                monitor.bounds.right - monitor.bounds.left,
                monitor.bounds.bottom - monitor.bounds.top,
            );
            if width <= 0 || height <= 0 {
                return Err("Selected display is unavailable".into());
            }
            let (source_x, source_y, source_width, source_height, cols, rows) =
                match self.capture.sampling {
                    ScreenSampling::Average => (
                        monitor.bounds.left,
                        monitor.bounds.top,
                        width,
                        height,
                        COLS,
                        ROWS,
                    ),
                    ScreenSampling::Point { x, y } => (
                        monitor.bounds.left + point_coordinate(x, width)?,
                        monitor.bounds.top + point_coordinate(y, height)?,
                        1,
                        1,
                        1,
                        1,
                    ),
                };
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
                    width: cols,
                    height: -rows, // top-down 32-bit BGRA
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
                    cols,
                    rows,
                    dc.0,
                    source_x,
                    source_y,
                    source_width,
                    source_height,
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
                std::slice::from_raw_parts(bits.cast::<u8>(), (cols * rows * 4) as usize)
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
    use super::{
        COLS, DisplaySource, ROWS, ScreenCapture, ScreenSampling, grid_coordinate, mean_rgb,
        point_coordinate, select_display,
    };
    use std::ffi::{CStr, CString, c_void};

    type Display = c_void;
    type Drawable = libc::c_ulong;
    type OpenDisplay = unsafe extern "C" fn(*const libc::c_char) -> *mut Display;
    type CloseDisplay = unsafe extern "C" fn(*mut Display) -> libc::c_int;
    type DefaultScreen = unsafe extern "C" fn(*mut Display) -> libc::c_int;
    type RootWindow = unsafe extern "C" fn(*mut Display, libc::c_int) -> Drawable;
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
    type GetMonitors = unsafe extern "C" fn(
        *mut Display,
        Drawable,
        libc::c_int,
        *mut libc::c_int,
    ) -> *mut XRRMonitorInfo;
    type FreeMonitors = unsafe extern "C" fn(*mut XRRMonitorInfo);
    type QueryVersion =
        unsafe extern "C" fn(*mut Display, *mut libc::c_int, *mut libc::c_int) -> libc::c_int;
    type DisplayExtent = unsafe extern "C" fn(*mut Display, libc::c_int) -> libc::c_int;
    type GetAtomName = unsafe extern "C" fn(*mut Display, libc::c_ulong) -> *mut libc::c_char;
    type Free = unsafe extern "C" fn(*mut c_void) -> libc::c_int;

    #[repr(C)]
    struct XRRMonitorInfo {
        name: libc::c_ulong,
        primary: libc::c_int,
        automatic: libc::c_int,
        noutput: libc::c_int,
        x: libc::c_int,
        y: libc::c_int,
        width: libc::c_int,
        height: libc::c_int,
        mwidth: libc::c_int,
        mheight: libc::c_int,
        outputs: *mut libc::c_ulong,
    }
    #[derive(Clone)]
    struct Monitor {
        source: DisplaySource,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        primary: bool,
    }

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
        atom_name: GetAtomName,
        free: Free,
        randr: Option<Randr>,
    }
    struct Randr {
        handle: *mut c_void,
        get: GetMonitors,
        free: FreeMonitors,
        query_version: QueryVersion,
    }
    impl Drop for Randr {
        fn drop(&mut self) {
            unsafe { libc::dlclose(self.handle) };
        }
    }
    impl Randr {
        fn load() -> Option<Self> {
            let handle = unsafe {
                libc::dlopen(
                    c"libXrandr.so.2".as_ptr(),
                    libc::RTLD_NOW | libc::RTLD_LOCAL,
                )
            };
            if handle.is_null() {
                return None;
            }
            let get = unsafe { libc::dlsym(handle, c"XRRGetMonitors".as_ptr()) };
            let free = unsafe { libc::dlsym(handle, c"XRRFreeMonitors".as_ptr()) };
            let query_version = unsafe { libc::dlsym(handle, c"XRRQueryVersion".as_ptr()) };
            if get.is_null() || free.is_null() || query_version.is_null() {
                unsafe { libc::dlclose(handle) };
                return None;
            }
            Some(Self {
                handle,
                get: unsafe { std::mem::transmute::<*mut c_void, GetMonitors>(get) },
                free: unsafe { std::mem::transmute::<*mut c_void, FreeMonitors>(free) },
                query_version: unsafe {
                    std::mem::transmute::<*mut c_void, QueryVersion>(query_version)
                },
            })
        }
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
                atom_name: sym!("XGetAtomName", GetAtomName),
                free: sym!("XFree", Free),
                randr: Randr::load(),
            })
        }
    }

    fn monitors(xlib: &Xlib, display: *mut Display) -> Result<Vec<Monitor>, String> {
        let screen = unsafe { (xlib.default_screen)(display) };
        let root = unsafe { (xlib.root)(display, screen) };
        let mut result = Vec::new();
        if let Some(randr) = &xlib.randr {
            let mut major = 0;
            let mut minor = 0;
            let supported = unsafe { (randr.query_version)(display, &mut major, &mut minor) } != 0
                && (major > 1 || (major == 1 && minor >= 5));
            if supported {
                let mut count = 0;
                let pointer = unsafe { (randr.get)(display, root, 1, &mut count) };
                if !pointer.is_null() && count > 0 {
                    for monitor in unsafe { std::slice::from_raw_parts(pointer, count as usize) } {
                        if monitor.width <= 0 || monitor.height <= 0 {
                            continue;
                        }
                        let name = unsafe { (xlib.atom_name)(display, monitor.name) };
                        if name.is_null() {
                            continue;
                        }
                        let id = unsafe { CStr::from_ptr(name) }
                            .to_string_lossy()
                            .into_owned();
                        unsafe { (xlib.free)(name.cast()) };
                        result.push(Monitor {
                            source: DisplaySource {
                                label: id.clone(),
                                id,
                            },
                            x: monitor.x,
                            y: monitor.y,
                            width: monitor.width,
                            height: monitor.height,
                            primary: monitor.primary != 0,
                        });
                    }
                }
                if !pointer.is_null() {
                    unsafe { (randr.free)(pointer) }
                }
            }
        }
        if result.is_empty() {
            let width = unsafe { (xlib.width)(display, screen) };
            let height = unsafe { (xlib.height)(display, screen) };
            if width <= 0 || height <= 0 {
                return Err("X11 primary screen is unavailable".into());
            }
            result.push(Monitor {
                source: DisplaySource {
                    id: "x11:root".into(),
                    label: "X11 screen".into(),
                },
                x: 0,
                y: 0,
                width,
                height,
                primary: true,
            });
        }
        Ok(result)
    }

    fn selected_monitor(
        xlib: &Xlib,
        display: *mut Display,
        id: Option<&str>,
    ) -> Result<Monitor, String> {
        let monitors = monitors(xlib, display)?;
        select_display(&monitors, id, |monitor| {
            (&monitor.source.id, monitor.primary)
        })
        .cloned()
        .ok_or_else(|| "Selected display is no longer available".into())
    }

    pub fn displays() -> Result<Vec<DisplaySource>, String> {
        let sampler = ScreenSampler::new()?;
        Ok(monitors(&sampler.xlib, sampler.display)?
            .into_iter()
            .map(|monitor| monitor.source)
            .collect())
    }

    pub struct ScreenSampler {
        xlib: Xlib,
        display: *mut Display,
        capture: ScreenCapture,
    }
    impl Drop for ScreenSampler {
        fn drop(&mut self) {
            unsafe { (self.xlib.close)(self.display) };
        }
    }
    impl ScreenSampler {
        pub fn new() -> Result<Self, String> {
            Self::with_capture(ScreenCapture::default())
        }

        pub fn with_capture(capture: ScreenCapture) -> Result<Self, String> {
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
            let monitor = selected_monitor(&xlib, display, capture.display_id.as_deref());
            if let Err(error) = monitor {
                unsafe { (xlib.close)(display) };
                return Err(error);
            }
            let monitor = monitor?;
            if let ScreenSampling::Point { x, y } = capture.sampling {
                if let Err(error) = point_coordinate(x, monitor.width)
                    .and_then(|_| point_coordinate(y, monitor.height))
                {
                    unsafe { (xlib.close)(display) };
                    return Err(error);
                }
            }
            Ok(Self {
                xlib,
                display,
                capture,
            })
        }

        pub fn sample(&mut self) -> Result<[u8; 3], String> {
            let screen = unsafe { (self.xlib.default_screen)(self.display) };
            let monitor =
                selected_monitor(&self.xlib, self.display, self.capture.display_id.as_deref())?;
            let (width, height) = (monitor.width, monitor.height);
            if width <= 0 || height <= 0 {
                return Err("Selected display is unavailable".into());
            }
            let root = unsafe { (self.xlib.root)(self.display, screen) };
            if let ScreenSampling::Point { x, y } = self.capture.sampling {
                let image = unsafe {
                    (self.xlib.image)(
                        self.display,
                        root,
                        monitor.x + point_coordinate(x, width)?,
                        monitor.y + point_coordinate(y, height)?,
                        1,
                        1,
                        !0,
                        2,
                    )
                };
                if image.is_null() {
                    return Err("X11 point capture failed or desktop is unavailable".into());
                }
                return mean_rgb([rgb_from_image(&Image(image), 0, 0)?]);
            }
            let mut colors = Vec::with_capacity((COLS * ROWS) as usize);
            for row in 0..ROWS {
                // Fetch one scanline rather than a full screen image. Nine
                // short X11 requests keep memory and transfer cost bounded.
                let image = unsafe {
                    (self.xlib.image)(
                        self.display,
                        root,
                        monitor.x,
                        monitor.y + grid_coordinate(row, ROWS, height),
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
                for col in 0..COLS {
                    colors.push(rgb_from_image(
                        &image,
                        grid_coordinate(col, COLS, width),
                        0,
                    )?);
                }
            }
            mean_rgb(colors)
        }
    }

    fn rgb_from_image(image: &Image, x: i32, y: i32) -> Result<[u8; 3], String> {
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
        let pixel = unsafe { ((*image.0).funcs.get_pixel)(image.0, x, y) };
        Ok(masks.map(|mask| {
            let shift = mask.trailing_zeros();
            let max = mask >> shift;
            (((pixel & mask) >> shift) * 255 / max) as u8
        }))
    }
}

pub use platform::{ScreenSampler, displays};

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
    #[test]
    fn maps_normalized_points_inside_display_bounds() {
        assert_eq!(point_coordinate(0, 1920).unwrap(), 0);
        assert_eq!(point_coordinate(500, 1920).unwrap(), 959);
        assert_eq!(point_coordinate(1000, 1920).unwrap(), 1919);
        assert_eq!(point_coordinate(1000, 1).unwrap(), 0);
        assert!(point_coordinate(1001, 1920).is_err());
        assert!(point_coordinate(0, 0).is_err());
    }
    #[test]
    fn selected_display_never_falls_back_for_a_stale_id() {
        let displays = [("secondary", false), ("primary", true)];
        assert_eq!(
            select_display(&displays, None, |item| (item.0, item.1)),
            Some(&displays[1])
        );
        assert_eq!(
            select_display(&displays, Some("secondary"), |item| (item.0, item.1)),
            Some(&displays[0])
        );
        assert_eq!(
            select_display(&displays, Some("removed"), |item| (item.0, item.1)),
            None
        );
    }
}
