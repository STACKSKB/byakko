//! Linux playback monitor sampling through the system's libpulse.so.0.
//!
//! This is an original FFI binding. The shared library is loaded from the
//! user's system at run time; no PulseAudio implementation is bundled.

use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    marker::PhantomData,
    ptr::{self, NonNull},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

const CONTEXT_READY: c_int = 4;
const CONTEXT_FAILED: c_int = 5;
const CONTEXT_TERMINATED: c_int = 6;
const STREAM_READY: c_int = 2;
const STREAM_FAILED: c_int = 3;
const STREAM_TERMINATED: c_int = 4;
const FLOAT32LE: c_int = 5;
const CONTEXT_NOAUTOSPAWN: c_int = 1;
const STREAM_DONT_MOVE: c_int = 0x0200;
const RATE: u32 = 48_000;
const STARTUP_LIMIT: Duration = Duration::from_secs(3);
const MAX_SAMPLE_BYTES: usize = 256 * 1024;
const MAX_SAMPLE_ROUNDS: usize = 8;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

// Prefixes of the public PulseAudio introspection structs through the fields
// this module reads. The trailing fields are intentionally never accessed.
#[repr(C)]
struct ServerInfo {
    user_name: *const c_char,
    host_name: *const c_char,
    server_version: *const c_char,
    server_name: *const c_char,
    sample_spec: SampleSpec,
    default_sink_name: *const c_char,
}

#[repr(C)]
struct ChannelMap {
    channels: u8,
    map: [c_int; 32],
}

#[repr(C)]
struct ChannelVolume {
    channels: u8,
    values: [u32; 32],
}

#[repr(C)]
struct SinkInfo {
    name: *const c_char,
    index: u32,
    description: *const c_char,
    sample_spec: SampleSpec,
    channel_map: ChannelMap,
    owner_module: u32,
    volume: ChannelVolume,
    mute: c_int,
    monitor_source: u32,
    monitor_source_name: *const c_char,
}

#[derive(Default)]
struct ServerReply {
    sink_name: Option<CString>,
    called: bool,
}
#[derive(Default)]
struct SinkReply {
    monitor_name: Option<CString>,
    monitor_index: Option<u32>,
    called: bool,
    failed: bool,
}

unsafe extern "C" fn on_server_info(_: *mut c_void, info: *const ServerInfo, user: *mut c_void) {
    let reply = unsafe { &mut *user.cast::<ServerReply>() };
    reply.called = true;
    if !info.is_null() {
        let name = unsafe { (*info).default_sink_name };
        if !name.is_null() {
            reply.sink_name = Some(unsafe { CStr::from_ptr(name) }.to_owned());
        }
    }
}

unsafe extern "C" fn on_sink_info(
    _: *mut c_void,
    info: *const SinkInfo,
    eol: c_int,
    user: *mut c_void,
) {
    let reply = unsafe { &mut *user.cast::<SinkReply>() };
    if eol < 0 {
        reply.failed = true;
        reply.called = true;
        return;
    }
    if eol > 0 {
        reply.called = true;
        return;
    }
    if !info.is_null() {
        let name = unsafe { (*info).monitor_source_name };
        let index = unsafe { (*info).monitor_source };
        if !name.is_null() && index != u32::MAX {
            reply.monitor_name = Some(unsafe { CStr::from_ptr(name) }.to_owned());
            reply.monitor_index = Some(index);
        }
    }
}

type ServerInfoCallback = unsafe extern "C" fn(*mut c_void, *const ServerInfo, *mut c_void);
type SinkInfoCallback = unsafe extern "C" fn(*mut c_void, *const SinkInfo, c_int, *mut c_void);
type GetServerInfo =
    unsafe extern "C" fn(*mut c_void, ServerInfoCallback, *mut c_void) -> *mut c_void;
type GetSinkInfoByName =
    unsafe extern "C" fn(*mut c_void, *const c_char, SinkInfoCallback, *mut c_void) -> *mut c_void;
type OperationState = unsafe extern "C" fn(*mut c_void) -> c_int;
type OperationCancel = unsafe extern "C" fn(*mut c_void);
type OperationUnref = unsafe extern "C" fn(*mut c_void);

type MainloopNew = unsafe extern "C" fn() -> *mut c_void;
type MainloopFree = unsafe extern "C" fn(*mut c_void);
type MainloopApi = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type MainloopIterate = unsafe extern "C" fn(*mut c_void, c_int, *mut c_int) -> c_int;
type ContextNew = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void;
type ContextConnect =
    unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *const c_void) -> c_int;
type ContextState = unsafe extern "C" fn(*mut c_void) -> c_int;
type ContextDisconnect = unsafe extern "C" fn(*mut c_void);
type ContextUnref = unsafe extern "C" fn(*mut c_void);
type ContextErrno = unsafe extern "C" fn(*mut c_void) -> c_int;
type Strerror = unsafe extern "C" fn(c_int) -> *const c_char;
type StreamNew = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    *const SampleSpec,
    *const c_void,
) -> *mut c_void;
type StreamConnectRecord =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const BufferAttr, c_int) -> c_int;
type StreamState = unsafe extern "C" fn(*mut c_void) -> c_int;
type StreamDisconnect = unsafe extern "C" fn(*mut c_void) -> c_int;
type StreamUnref = unsafe extern "C" fn(*mut c_void);
type StreamReadableSize = unsafe extern "C" fn(*mut c_void) -> usize;
type StreamPeek = unsafe extern "C" fn(*mut c_void, *mut *const c_void, *mut usize) -> c_int;
type StreamDrop = unsafe extern "C" fn(*mut c_void) -> c_int;
type StreamDeviceIndex = unsafe extern "C" fn(*mut c_void) -> u32;

struct Pulse {
    library: NonNull<c_void>,
    mainloop_new: MainloopNew,
    mainloop_free: MainloopFree,
    mainloop_api: MainloopApi,
    mainloop_iterate: MainloopIterate,
    context_new: ContextNew,
    context_connect: ContextConnect,
    context_state: ContextState,
    context_disconnect: ContextDisconnect,
    context_unref: ContextUnref,
    context_errno: ContextErrno,
    strerror: Strerror,
    get_server_info: GetServerInfo,
    get_sink_info_by_name: GetSinkInfoByName,
    operation_state: OperationState,
    operation_cancel: OperationCancel,
    operation_unref: OperationUnref,
    stream_new: StreamNew,
    stream_connect_record: StreamConnectRecord,
    stream_state: StreamState,
    stream_disconnect: StreamDisconnect,
    stream_unref: StreamUnref,
    stream_readable_size: StreamReadableSize,
    stream_peek: StreamPeek,
    stream_drop: StreamDrop,
    stream_device_index: StreamDeviceIndex,
}

impl Pulse {
    fn load() -> Result<Self, String> {
        // RTLD_NOW resolves missing symbols immediately. libpulse is a system
        // dependency, loaded only when Linux music lighting is started.
        let library =
            unsafe { libc::dlopen(c"libpulse.so.0".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        let library = NonNull::new(library)
            .ok_or("libpulse.so.0 is unavailable; install PulseAudio client libraries")?;
        unsafe {
            // Close the library on partial symbol resolution failure too.
            let build = || -> Result<Self, String> {
                macro_rules! sym {
                    ($name:literal, $type:ty) => {{
                        let address =
                            libc::dlsym(library.as_ptr(), concat!($name, "\0").as_ptr().cast());
                        if address.is_null() {
                            return Err(format!("libpulse.so.0 lacks {}", $name));
                        }
                        std::mem::transmute::<*mut c_void, $type>(address)
                    }};
                }
                Ok(Self {
                    library,
                    mainloop_new: sym!("pa_mainloop_new", MainloopNew),
                    mainloop_free: sym!("pa_mainloop_free", MainloopFree),
                    mainloop_api: sym!("pa_mainloop_get_api", MainloopApi),
                    mainloop_iterate: sym!("pa_mainloop_iterate", MainloopIterate),
                    context_new: sym!("pa_context_new", ContextNew),
                    context_connect: sym!("pa_context_connect", ContextConnect),
                    context_state: sym!("pa_context_get_state", ContextState),
                    context_disconnect: sym!("pa_context_disconnect", ContextDisconnect),
                    context_unref: sym!("pa_context_unref", ContextUnref),
                    context_errno: sym!("pa_context_errno", ContextErrno),
                    strerror: sym!("pa_strerror", Strerror),
                    get_server_info: sym!("pa_context_get_server_info", GetServerInfo),
                    get_sink_info_by_name: sym!(
                        "pa_context_get_sink_info_by_name",
                        GetSinkInfoByName
                    ),
                    operation_state: sym!("pa_operation_get_state", OperationState),
                    operation_cancel: sym!("pa_operation_cancel", OperationCancel),
                    operation_unref: sym!("pa_operation_unref", OperationUnref),
                    stream_new: sym!("pa_stream_new", StreamNew),
                    stream_connect_record: sym!("pa_stream_connect_record", StreamConnectRecord),
                    stream_state: sym!("pa_stream_get_state", StreamState),
                    stream_disconnect: sym!("pa_stream_disconnect", StreamDisconnect),
                    stream_unref: sym!("pa_stream_unref", StreamUnref),
                    stream_readable_size: sym!("pa_stream_readable_size", StreamReadableSize),
                    stream_peek: sym!("pa_stream_peek", StreamPeek),
                    stream_drop: sym!("pa_stream_drop", StreamDrop),
                    stream_device_index: sym!("pa_stream_get_device_index", StreamDeviceIndex),
                })
            };
            let result = build();
            if result.is_err() {
                libc::dlclose(library.as_ptr());
            }
            result
        }
    }

    fn error(&self, context: *mut c_void, operation: &str) -> String {
        unsafe {
            let ptr = (self.strerror)((self.context_errno)(context));
            let message = if ptr.is_null() {
                "unknown PulseAudio error".into()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            };
            format!("{operation}: {message}")
        }
    }
}

impl Drop for Pulse {
    fn drop(&mut self) {
        unsafe {
            libc::dlclose(self.library.as_ptr());
        }
    }
}

pub struct AudioSampler {
    pulse: Pulse,
    mainloop: *mut c_void,
    context: *mut c_void,
    stream: *mut c_void,
    monitor_index: u32,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl AudioSampler {
    pub fn new() -> Result<Self, String> {
        let pulse = Pulse::load()?;
        let mut self_ = Self {
            mainloop: ptr::null_mut(),
            context: ptr::null_mut(),
            stream: ptr::null_mut(),
            monitor_index: u32::MAX,
            pulse,
            _thread_affinity: PhantomData,
        };
        unsafe {
            self_.mainloop = (self_.pulse.mainloop_new)();
            if self_.mainloop.is_null() {
                return Err("pa_mainloop_new failed".into());
            }
            let api = (self_.pulse.mainloop_api)(self_.mainloop);
            if api.is_null() {
                return Err("pa_mainloop_get_api failed".into());
            }
            self_.context = (self_.pulse.context_new)(api, c"Byakko music lighting".as_ptr());
            if self_.context.is_null() {
                return Err("pa_context_new failed".into());
            }
            if (self_.pulse.context_connect)(
                self_.context,
                ptr::null(),
                CONTEXT_NOAUTOSPAWN,
                ptr::null(),
            ) < 0
            {
                return Err(self_.pulse.error(self_.context, "pa_context_connect"));
            }
            self_.wait_ready(false)?;
            let (monitor_name, monitor_index) = self_.discover_monitor()?;
            self_.monitor_index = monitor_index;
            let spec = SampleSpec {
                format: FLOAT32LE,
                rate: RATE,
                channels: 1,
            };
            self_.stream = (self_.pulse.stream_new)(
                self_.context,
                c"Byakko playback monitor".as_ptr(),
                &spec,
                ptr::null(),
            );
            if self_.stream.is_null() {
                return Err(self_.pulse.error(self_.context, "pa_stream_new"));
            }
            // Request ~30 ms fragments. The server may select another size.
            let attr = BufferAttr {
                maxlength: u32::MAX,
                tlength: u32::MAX,
                prebuf: u32::MAX,
                minreq: u32::MAX,
                fragsize: RATE * 4 * 30 / 1000,
            };
            if (self_.pulse.stream_connect_record)(
                self_.stream,
                monitor_name.as_ptr(),
                &attr,
                STREAM_DONT_MOVE,
            ) < 0
            {
                return Err(self_
                    .pulse
                    .error(self_.context, "pa_stream_connect_record(playback monitor)"));
            }
            self_.wait_ready(true)?;
            self_.verify_monitor()?;
        }
        Ok(self_)
    }

    fn discover_monitor(&mut self) -> Result<(CString, u32), String> {
        let mut server = ServerReply::default();
        let operation = unsafe {
            (self.pulse.get_server_info)(self.context, on_server_info, (&raw mut server).cast())
        };
        self.run_operation(operation, "get default playback sink")?;
        let sink_name = server
            .sink_name
            .ok_or("PulseAudio has no default playback sink")?;
        let mut sink = SinkReply::default();
        let operation = unsafe {
            (self.pulse.get_sink_info_by_name)(
                self.context,
                sink_name.as_ptr(),
                on_sink_info,
                (&raw mut sink).cast(),
            )
        };
        self.run_operation(operation, "get playback monitor")?;
        if sink.failed {
            return Err(self.pulse.error(self.context, "get playback monitor"));
        }
        let name = sink
            .monitor_name
            .ok_or("default playback sink has no monitor source")?;
        let index = sink
            .monitor_index
            .ok_or("default playback sink has no monitor index")?;
        Ok((name, index))
    }

    fn run_operation(&mut self, operation: *mut c_void, description: &str) -> Result<(), String> {
        if operation.is_null() {
            return Err(self.pulse.error(self.context, description));
        }
        let deadline = Instant::now() + STARTUP_LIMIT;
        let result = loop {
            let state = unsafe { (self.pulse.operation_state)(operation) };
            if state == 1 {
                break Ok(());
            } // PA_OPERATION_DONE
            if state == 2 {
                break Err(format!("{description}: operation cancelled"));
            }
            let context_state = unsafe { (self.pulse.context_state)(self.context) };
            if context_state != CONTEXT_READY {
                break Err(self.pulse.error(self.context, description));
            }
            if Instant::now() >= deadline {
                break Err(format!("{description}: operation timed out"));
            }
            if let Err(err) = self.iterate() {
                break Err(err);
            }
            thread::sleep(Duration::from_millis(5));
        };
        unsafe {
            if result.is_err() && (self.pulse.operation_state)(operation) == 0 {
                (self.pulse.operation_cancel)(operation);
            }
            (self.pulse.operation_unref)(operation);
        }
        result
    }

    fn verify_monitor(&self) -> Result<(), String> {
        let device = unsafe { (self.pulse.stream_device_index)(self.stream) };
        if device != self.monitor_index {
            Err(format!(
                "PulseAudio record stream routed to unexpected source {device}; expected output monitor {}",
                self.monitor_index
            ))
        } else {
            Ok(())
        }
    }

    fn wait_ready(&mut self, stream: bool) -> Result<(), String> {
        let deadline = Instant::now() + STARTUP_LIMIT;
        loop {
            let state = unsafe {
                if stream {
                    (self.pulse.stream_state)(self.stream)
                } else {
                    (self.pulse.context_state)(self.context)
                }
            };
            if state == if stream { STREAM_READY } else { CONTEXT_READY } {
                return Ok(());
            }
            if state
                == if stream {
                    STREAM_FAILED
                } else {
                    CONTEXT_FAILED
                }
                || state
                    == if stream {
                        STREAM_TERMINATED
                    } else {
                        CONTEXT_TERMINATED
                    }
            {
                return Err(self.pulse.error(
                    self.context,
                    if stream {
                        "PulseAudio monitor stream"
                    } else {
                        "PulseAudio server connection"
                    },
                ));
            }
            if Instant::now() >= deadline {
                return Err(if stream {
                    "PulseAudio monitor stream startup timed out"
                } else {
                    "PulseAudio server connection timed out"
                }
                .into());
            }
            self.iterate()?;
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn iterate(&mut self) -> Result<(), String> {
        let result = unsafe { (self.pulse.mainloop_iterate)(self.mainloop, 0, ptr::null_mut()) };
        if result < 0 {
            Err("pa_mainloop_iterate failed".into())
        } else {
            Ok(())
        }
    }

    pub fn sample_rate(&self) -> u32 {
        RATE
    }

    /// Process pending events without blocking and drain at most eight packets.
    pub fn sample(&mut self) -> Result<Vec<f32>, String> {
        for _ in 0..4 {
            self.iterate()?;
        }
        let state = unsafe { (self.pulse.stream_state)(self.stream) };
        if state != STREAM_READY {
            return Err(self
                .pulse
                .error(self.context, "PulseAudio monitor stream disconnected"));
        }
        self.verify_monitor()?;
        let mut out = Vec::new();
        for _ in 0..MAX_SAMPLE_ROUNDS {
            let available = unsafe { (self.pulse.stream_readable_size)(self.stream) };
            if available == 0 {
                break;
            }
            if available == usize::MAX {
                return Err(self.pulse.error(self.context, "pa_stream_readable_size"));
            }
            let (mut data, mut bytes) = (ptr::null(), 0usize);
            if unsafe { (self.pulse.stream_peek)(self.stream, &mut data, &mut bytes) } < 0 {
                return Err(self.pulse.error(self.context, "pa_stream_peek"));
            }
            // No fragment was exposed, so there is nothing to release.
            if bytes == 0 {
                break;
            }
            let decode = if bytes > MAX_SAMPLE_BYTES || bytes % 4 != 0 {
                Err(format!(
                    "PulseAudio monitor packet has invalid size: {bytes}"
                ))
            } else if out.len().saturating_add(bytes / 4) > MAX_SAMPLE_BYTES / 4 {
                Err("PulseAudio monitor drain exceeded size bound".into())
            } else {
                if data.is_null() {
                    out.resize(out.len() + bytes / 4, 0.0);
                } else {
                    let raw = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), bytes) };
                    for chunk in raw.as_chunks::<4>().0 {
                        let value = f32::from_le_bytes(*chunk);
                        out.push(if value.is_finite() {
                            value.clamp(-1.0, 1.0)
                        } else {
                            0.0
                        });
                    }
                }
                Ok(())
            };
            let dropped = unsafe { (self.pulse.stream_drop)(self.stream) };
            decode?;
            if dropped < 0 {
                return Err(self.pulse.error(self.context, "pa_stream_drop"));
            }
        }
        Ok(out)
    }
}

impl Drop for AudioSampler {
    fn drop(&mut self) {
        unsafe {
            if !self.stream.is_null() {
                (self.pulse.stream_disconnect)(self.stream);
                (self.pulse.stream_unref)(self.stream);
            }
            if !self.context.is_null() {
                (self.pulse.context_disconnect)(self.context);
                (self.pulse.context_unref)(self.context);
            }
            if !self.mainloop.is_null() {
                (self.pulse.mainloop_free)(self.mainloop);
            }
        }
    }
}
