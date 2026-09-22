# Native playback-reactive lighting

2026-09-22, Nia87 USB3151:4015, firmware0100, profile0.

The product now captures the default Windows multimedia render endpoint with WASAPI loopback. It never opens a microphone endpoint, saves recordings, or sends samples over a network. COM references and apartment state are held and released on the worker thread. Each poll drains at most32 available packets; it does not wait for incoming audio. Supported mix formats are float32 and signed PCM16/24/32, including24 valid left-aligned bits in a32-bit container. Invalid formats are rejected before a keyboard mode change. Tests cover interleaved mono conversion, silence, nonfinite floats, packed format offsets, and unsupported formats.

Implementation uses original Rust/COM bindings to documented OS interfaces, with no additional product dependency. API references: [Microsoft default render endpoint](https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-getdefaultaudioendpoint), [IAudioCaptureClient](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-iaudiocaptureclient). No vendor or copyleft implementation was copied.

An original2048-sample Hann-windowed logarithmic frequency-probe bank produces32 levels from0–6, with immediate attack and gradual release. This is a lighting visualization, not an exact reproduction of the official spectrum algorithm. Unit tests exercise silence, a known tone, release back to zero, and invalid samples/rates.

The Lighting page now provides START AUDIO LIGHTING for music effects20/22, and a shared STOP HOST LIGHTING / RESTORE control. The session holds the existing interprocess HID lock through setup, streaming and verified restoration. Capture is opt-in and ends when stopped or during normal application close. Forced termination can prevent restoration. OS endpoint removal and sampler failures are reported; automatic endpoint switching is not implemented.

## Hardware evidence

`examples/verify_audio_stream.rs` ran the actual playback sampler, analyzer and music-mode22 worker for12seconds. An original in-memory synthesized480Hz tone played for6seconds at4% peak amplitude with short fades. System volume was not changed. The camera observed a green/cyan lit region across the right-hand keys: ignored `captures/keyboard-camera-1790051753598367900.png`. This verifies a nonzero playback signal through the complete native path, rather than only synthetic report encoding. A concurrent configuration read was rejected by the stream's lock. Saved Ripple effect parameters, complete scalar settings and both keymaps matched after stop.

The earlier idle read-only probe reported48kHz and zero frames, consistent with no active playback. Tone generation and camera capture are research-only scripts, not product dependencies. No recorded audio file was created.

Remaining: Linux playback capture, sustained/endpoint-change testing, GUI Start/Stop/close interaction verification, broader music patterns and stereo phase-cancellation behavior. Native Linux code currently returns an explicit unsupported capture error before changing the keyboard mode.
