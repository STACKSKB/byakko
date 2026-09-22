//! Native, read-only system playback sampling for host lighting.
//!
//! Create and use `AudioSampler` on the same worker thread. On Windows it
//! captures the default render endpoint through WASAPI loopback. Samples are
//! averaged to mono and never saved or sent anywhere by this module.

#[cfg(not(target_os = "windows"))]
pub struct AudioSampler;

#[cfg(not(target_os = "windows"))]
impl AudioSampler {
    pub fn new() -> Result<Self, String> {
        Err("system playback capture is not available on this platform yet".into())
    }
    pub fn sample(&mut self) -> Result<Vec<f32>, String> {
        Err("system playback capture is not available on this platform yet".into())
    }
    pub fn sample_rate(&self) -> u32 {
        0
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{
        ffi::c_void,
        marker::PhantomData,
        ptr::{null, null_mut},
        rc::Rc,
    };

    type Hr = i32;
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        a: u32,
        b: u16,
        c: u16,
        d: [u8; 8],
    }
    const ENUMERATOR_CLASS: Guid = Guid {
        a: 0xbcde0395,
        b: 0xe52f,
        c: 0x467c,
        d: [0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e],
    };
    const ENUMERATOR_IID: Guid = Guid {
        a: 0xa95664d2,
        b: 0x9614,
        c: 0x4f35,
        d: [0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6],
    };
    const CLIENT_IID: Guid = Guid {
        a: 0x1cb9ad4c,
        b: 0xdbfa,
        c: 0x4c32,
        d: [0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2],
    };
    const CAPTURE_IID: Guid = Guid {
        a: 0xc8adbd64,
        b: 0xe71e,
        c: 0x48a0,
        d: [0xa4, 0xde, 0x18, 0x5c, 0x39, 0x5c, 0xd3, 0x17],
    };
    const FLOAT_SUBFORMAT: Guid = Guid {
        a: 3,
        b: 0,
        c: 0x10,
        d: [0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71],
    };
    const PCM_SUBFORMAT: Guid = Guid {
        a: 1,
        b: 0,
        c: 0x10,
        d: [0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71],
    };
    const LOOPBACK: u32 = 0x0002_0000;
    const SILENT: u32 = 0x0000_0002;
    const CLSCTX_INPROC_SERVER: u32 = 1;

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> Hr;
        fn CoUninitialize();
        fn CoCreateInstance(
            class: *const Guid,
            outer: *mut c_void,
            context: u32,
            iid: *const Guid,
            out: *mut *mut c_void,
        ) -> Hr;
        fn CoTaskMemFree(ptr: *mut c_void);
    }

    // WAVEFORMATEX has an 18-byte wire header. Its variable cbSize payload
    // starts at byte 18; a Rust repr(C) structure would be rounded to 20.
    struct WaveFormatInfo {
        channels: usize,
        rate: u32,
        align: usize,
        encoding: Encoding,
    }

    struct Com(*mut c_void);
    impl Drop for Com {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                        std::mem::transmute(vmethod(self.0, 2));
                    release(self.0);
                }
            }
        }
    }
    struct MixFormat(*mut c_void);
    impl Drop for MixFormat {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CoTaskMemFree(self.0.cast()) }
            }
        }
    }
    unsafe fn vmethod(object: *mut c_void, index: usize) -> *mut c_void {
        unsafe { *((*(object as *mut *mut *mut c_void)).add(index)) }
    }
    fn check(hr: Hr, operation: &str) -> Result<(), String> {
        if hr < 0 {
            Err(format!("{operation} failed: HRESULT 0x{:08x}", hr as u32))
        } else {
            Ok(())
        }
    }
    struct Apartment;
    impl Apartment {
        fn new() -> Result<Self, String> {
            check(unsafe { CoInitializeEx(null_mut(), 0) }, "CoInitializeEx")?;
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }

    #[derive(Clone, Copy)]
    enum Encoding {
        Float,
        Signed16,
        Signed24,
        Signed32,
        Signed24In32,
    }

    pub struct AudioSampler {
        // COM references drop before the apartment, in declaration order.
        capture: Com,
        client: Com,
        _device: Com,
        _enumerator: Com,
        _apartment: Apartment,
        channels: usize,
        align: usize,
        rate: u32,
        encoding: Encoding,
        _thread_affinity: PhantomData<Rc<()>>,
    }

    impl AudioSampler {
        pub fn new() -> Result<Self, String> {
            let apartment = Apartment::new()?;
            unsafe {
                let mut enumerator = null_mut();
                check(
                    CoCreateInstance(
                        &ENUMERATOR_CLASS,
                        null_mut(),
                        CLSCTX_INPROC_SERVER,
                        &ENUMERATOR_IID,
                        &mut enumerator,
                    ),
                    "CoCreateInstance(MMDeviceEnumerator)",
                )?;
                let enumerator = Com(enumerator);
                let mut device = null_mut();
                let get_default: unsafe extern "system" fn(
                    *mut c_void,
                    i32,
                    i32,
                    *mut *mut c_void,
                ) -> Hr = std::mem::transmute(vmethod(enumerator.0, 4));
                check(
                    get_default(enumerator.0, 0, 1, &mut device),
                    "GetDefaultAudioEndpoint(render)",
                )?;
                let device = Com(device);
                let mut client = null_mut();
                let activate: unsafe extern "system" fn(
                    *mut c_void,
                    *const Guid,
                    u32,
                    *mut c_void,
                    *mut *mut c_void,
                ) -> Hr = std::mem::transmute(vmethod(device.0, 3));
                check(
                    activate(
                        device.0,
                        &CLIENT_IID,
                        CLSCTX_INPROC_SERVER,
                        null_mut(),
                        &mut client,
                    ),
                    "IMMDevice::Activate(IAudioClient)",
                )?;
                let client = Com(client);
                let mut format_ptr: *mut c_void = null_mut();
                let mix: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> Hr =
                    std::mem::transmute(vmethod(client.0, 8));
                check(mix(client.0, &mut format_ptr), "IAudioClient::GetMixFormat")?;
                if format_ptr.is_null() {
                    return Err("GetMixFormat returned no format".into());
                }
                let mix_format = MixFormat(format_ptr);
                let format = parse_format(mix_format.0)?;
                let initialize: unsafe extern "system" fn(
                    *mut c_void,
                    i32,
                    u32,
                    i64,
                    i64,
                    *const c_void,
                    *const Guid,
                ) -> Hr = std::mem::transmute(vmethod(client.0, 3));
                let hr = initialize(client.0, 0, LOOPBACK, 0, 0, mix_format.0, null());
                check(hr, "IAudioClient::Initialize(loopback)")?;
                let mut capture = null_mut();
                let service: unsafe extern "system" fn(
                    *mut c_void,
                    *const Guid,
                    *mut *mut c_void,
                ) -> Hr = std::mem::transmute(vmethod(client.0, 14));
                check(
                    service(client.0, &CAPTURE_IID, &mut capture),
                    "IAudioClient::GetService(capture)",
                )?;
                let capture = Com(capture);
                let start: unsafe extern "system" fn(*mut c_void) -> Hr =
                    std::mem::transmute(vmethod(client.0, 10));
                check(start(client.0), "IAudioClient::Start")?;
                Ok(Self {
                    capture,
                    client,
                    _device: device,
                    _enumerator: enumerator,
                    _apartment: apartment,
                    channels: format.channels,
                    align: format.align,
                    rate: format.rate,
                    encoding: format.encoding,
                    _thread_affinity: PhantomData,
                })
            }
        }

        pub fn sample_rate(&self) -> u32 {
            self.rate
        }

        /// Drain at most 32 currently queued packets, without waiting for audio.
        pub fn sample(&mut self) -> Result<Vec<f32>, String> {
            let mut samples = Vec::new();
            unsafe {
                let next: unsafe extern "system" fn(*mut c_void, *mut u32) -> Hr =
                    std::mem::transmute(vmethod(self.capture.0, 5));
                let get: unsafe extern "system" fn(
                    *mut c_void,
                    *mut *mut u8,
                    *mut u32,
                    *mut u32,
                    *mut u64,
                    *mut u64,
                ) -> Hr = std::mem::transmute(vmethod(self.capture.0, 3));
                let release: unsafe extern "system" fn(*mut c_void, u32) -> Hr =
                    std::mem::transmute(vmethod(self.capture.0, 4));
                for _ in 0..32 {
                    let mut available = 0;
                    check(next(self.capture.0, &mut available), "GetNextPacketSize")?;
                    if available == 0 {
                        break;
                    }
                    let (mut ptr, mut frames, mut flags) = (null_mut(), 0, 0);
                    check(
                        get(
                            self.capture.0,
                            &mut ptr,
                            &mut frames,
                            &mut flags,
                            null_mut(),
                            null_mut(),
                        ),
                        "IAudioCaptureClient::GetBuffer",
                    )?;
                    let result = self.append_packet(ptr, frames, flags, &mut samples);
                    let release_result = check(
                        release(self.capture.0, frames),
                        "IAudioCaptureClient::ReleaseBuffer",
                    );
                    result?;
                    release_result?;
                }
            }
            Ok(samples)
        }

        fn append_packet(
            &self,
            ptr: *mut u8,
            frames: u32,
            flags: u32,
            out: &mut Vec<f32>,
        ) -> Result<(), String> {
            let len = frames as usize;
            if len > 1_000_000 {
                return Err("WASAPI packet size exceeded bound".into());
            }
            if flags & SILENT != 0 {
                append_silence(out, len);
                return Ok(());
            }
            if ptr.is_null() {
                return Err("WASAPI returned null audio buffer".into());
            }
            let byte_len = len
                .checked_mul(self.align)
                .ok_or("WASAPI packet length overflow")?;
            let data = unsafe { std::slice::from_raw_parts(ptr, byte_len) };
            out.extend(decode_frames(
                data,
                self.channels,
                self.align,
                self.encoding,
            ));
            Ok(())
        }
    }

    impl Drop for AudioSampler {
        fn drop(&mut self) {
            unsafe {
                let stop: unsafe extern "system" fn(*mut c_void) -> Hr =
                    std::mem::transmute(vmethod(self.client.0, 11));
                let _ = stop(self.client.0);
            }
        }
    }

    unsafe fn parse_format(ptr: *const c_void) -> Result<WaveFormatInfo, String> {
        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), 18) };
        let tag = u16::from_le_bytes([bytes[0], bytes[1]]);
        let channels = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
        let rate = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let align = u16::from_le_bytes([bytes[12], bytes[13]]) as usize;
        let bits = u16::from_le_bytes([bytes[14], bytes[15]]);
        let extra = u16::from_le_bytes([bytes[16], bytes[17]]);
        let (tag, valid_bits) = if tag == 0xfffe {
            if extra < 22 {
                return Err("short WAVEFORMATEXTENSIBLE".into());
            }
            let extension = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>().add(18), 22) };
            let valid_bits = u16::from_le_bytes([extension[0], extension[1]]);
            let subtype =
                unsafe { std::ptr::read_unaligned(extension[6..].as_ptr().cast::<Guid>()) };
            let tag = if subtype.a == FLOAT_SUBFORMAT.a
                && subtype.b == FLOAT_SUBFORMAT.b
                && subtype.c == FLOAT_SUBFORMAT.c
                && subtype.d == FLOAT_SUBFORMAT.d
            {
                3
            } else if subtype.a == PCM_SUBFORMAT.a
                && subtype.b == PCM_SUBFORMAT.b
                && subtype.c == PCM_SUBFORMAT.c
                && subtype.d == PCM_SUBFORMAT.d
            {
                1
            } else {
                return Err("unsupported WASAPI subformat".into());
            };
            (tag, valid_bits)
        } else {
            (tag, bits)
        };
        let encoding = match (tag, bits, valid_bits) {
            (3, 32, 32) => Encoding::Float,
            (1, 16, 16) => Encoding::Signed16,
            (1, 24, 24) => Encoding::Signed24,
            (1, 32, 32) => Encoding::Signed32,
            (1, 32, 24) => Encoding::Signed24In32,
            _ => {
                return Err(format!(
                    "unsupported WASAPI format tag {tag}, {bits} container bits, {valid_bits} valid bits"
                ));
            }
        };
        if channels == 0 || channels > 32 || rate == 0 || align != channels * (bits as usize / 8) {
            return Err("unsupported WASAPI channel or frame layout".into());
        }
        Ok(WaveFormatInfo {
            channels,
            rate,
            align,
            encoding,
        })
    }

    fn decode_frames(data: &[u8], channels: usize, align: usize, encoding: Encoding) -> Vec<f32> {
        let mut out = Vec::with_capacity(data.len() / align);
        let width = align / channels;
        for frame in data.chunks_exact(align) {
            let mut sum = 0.0f32;
            for channel in frame.chunks_exact(width) {
                sum += match encoding {
                    Encoding::Float => {
                        let x = f32::from_le_bytes(channel.try_into().unwrap());
                        if x.is_finite() {
                            x.clamp(-1.0, 1.0)
                        } else {
                            0.0
                        }
                    }
                    Encoding::Signed16 => {
                        i16::from_le_bytes(channel.try_into().unwrap()) as f32 / 32768.0
                    }
                    Encoding::Signed24 => {
                        let n = i32::from_le_bytes([
                            channel[0],
                            channel[1],
                            channel[2],
                            if channel[2] & 0x80 != 0 { 0xff } else { 0 },
                        ]);
                        n as f32 / 8388608.0
                    }
                    Encoding::Signed32 => {
                        i32::from_le_bytes(channel.try_into().unwrap()) as f32 / 2147483648.0
                    }
                    // WAVEFORMATEXTENSIBLE valid PCM bits are left-aligned.
                    Encoding::Signed24In32 => {
                        (i32::from_le_bytes(channel.try_into().unwrap()) >> 8) as f32 / 8388608.0
                    }
                };
            }
            out.push((sum / channels as f32).clamp(-1.0, 1.0));
        }
        out
    }

    fn append_silence(out: &mut Vec<f32>, frames: usize) {
        out.resize(out.len() + frames, 0.0);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn converts_interleaved_pcm_to_mono() {
            let pcm16 = [0xff, 0x7f, 0x00, 0x80];
            assert!(decode_frames(&pcm16, 2, 4, Encoding::Signed16)[0].abs() < 0.00002);
            let pcm24 = [0xff, 0xff, 0x7f, 0, 0, 0x80];
            assert!(decode_frames(&pcm24, 2, 6, Encoding::Signed24)[0].abs() < 0.000001);
            let pcm32 = [0xff, 0xff, 0xff, 0x7f, 0, 0, 0, 0x80];
            assert!(decode_frames(&pcm32, 2, 8, Encoding::Signed32)[0].abs() < 0.000001);
            let pcm24in32 = [0, 0xff, 0xff, 0x7f, 0, 0, 0, 0x80];
            assert!(decode_frames(&pcm24in32, 2, 8, Encoding::Signed24In32)[0].abs() < 0.000001);
        }

        #[test]
        fn float_nonfinite_and_silent_packets_are_zero() {
            let mut data = Vec::new();
            data.extend_from_slice(&f32::NAN.to_le_bytes());
            data.extend_from_slice(&1.0f32.to_le_bytes());
            let mut out = decode_frames(&data, 2, 8, Encoding::Float);
            assert_eq!(out, [0.5]);
            append_silence(&mut out, 3);
            assert_eq!(out, [0.5, 0.0, 0.0, 0.0]);
        }

        #[test]
        fn extensible_header_uses_byte_18_for_valid_bits() {
            let mut raw = [0u8; 40];
            raw[0..2].copy_from_slice(&0xfffeu16.to_le_bytes());
            raw[2..4].copy_from_slice(&2u16.to_le_bytes());
            raw[4..8].copy_from_slice(&48000u32.to_le_bytes());
            raw[12..14].copy_from_slice(&8u16.to_le_bytes());
            raw[14..16].copy_from_slice(&32u16.to_le_bytes());
            raw[16..18].copy_from_slice(&22u16.to_le_bytes());
            raw[18..20].copy_from_slice(&24u16.to_le_bytes());
            let subtype = PCM_SUBFORMAT;
            raw[24..28].copy_from_slice(&subtype.a.to_le_bytes());
            raw[28..30].copy_from_slice(&subtype.b.to_le_bytes());
            raw[30..32].copy_from_slice(&subtype.c.to_le_bytes());
            raw[32..40].copy_from_slice(&subtype.d);
            let info = unsafe { parse_format(raw.as_ptr().cast()) }.unwrap();
            assert_eq!(info.rate, 48000);
            assert!(matches!(info.encoding, Encoding::Signed24In32));
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::AudioSampler;
