# Host lighting: captured transport and visible native output

Date: 2026-09-22. Stock Nia87 USB VID3151/PID4015, firmware0100, profile0. The official helper was stopped before every native test. No firmware was flashed.

The first synthetic test used the earlier incorrect static-trace opcodes and produced no visible LEDs. The official app's Light Shadow mode then produced captured `HidD_SetFeature` reports (length67 including report ID and two trailing zeros) with payload header `0e df df df 00 00 00 54`. The camera showed illumination. Music Follow2 selected mode22 and repeatedly sent silent frames beginning `0d 00 00 00 00 00 00 f2`. Capture: ignored `captures/host-lighting-final-2.log`. This disproved the generic-ancestor opcode assumption; `host-frame-correction.md` records the effective ancestry.

The corrected original encoders use screen opcode0E with RGB at bytes1–3 and music opcode0D with32 values at8–39, both BIT7 checksum. Our native65-byte host reports worked: no67-byte transport was needed. `host_frames_camera_check` produced visibly red then blue screen lighting in mode21, no visible light for zero music bands, and red lighting for all15 music bands in mode20. Images:

- screen red: `captures/keyboard-camera-1790050726240963000.png`
- screen blue: `captures/keyboard-camera-1790050730236817700.png`
- music zero: `captures/keyboard-camera-1790050735180967800.png`
- music15: `captures/keyboard-camera-1790050739097012300.png`

These establish command transport and response to synthetic frames, not audio analysis or all music patterns. Final effect parameters, complete settings and both keymaps matched their original state after restoration.

## Native screen sampler

The product now has opt-in screen color streaming from the Lighting page. Start samples the primary display locally, holds the existing interprocess device lock across mode selection and streaming, and temporarily selects mode21. Stop restores the previous effect with readback. Normal window close requests stop and wait for the worker to finish restoration; restoration errors keep the app open with an error. Abrupt process termination cannot guarantee restoration.

Windows uses a16×9 GDI DIB with StretchBlt and averages RGB. An initial144-GetPixel implementation took2.4–4seconds per sample; it was replaced before integration. The read-only optimized probe measured15–32ms per sample on this host. Screen and camera data are not sent over a network; the product does not save screen images. The separate research camera script saves keyboard stills only when invoked explicitly.

`verify_screen_stream` ran the actual sampler/stream worker for10seconds. A competing read was refused while streaming, camera image `captures/keyboard-camera-1790051207588422800.png` showed illuminated keys, and saved lighting, all settings and both keymaps matched after stopping. The first version's3second lock assertion ran before the slow sampler acquired the session; the optimized implementation passed. This is a worker/device integration check; the GUI's Start/Stop and close-restoration interactions have not been verified end to end. Native UI screenshots currently fail in the computer-use tool with Windows capture interface error0x80004002.

Linux includes a dynamically loaded X11 sampler and rejects Wayland sessions explicitly. Linux compilation/linking is checked separately; runtime capture and server-disconnect behavior remain unverified. Xlib's default fatal connection handler remains a known limitation. Native audio sampling, Wayland portal capture, sustained streaming and disconnect/reconnect recovery remain open.

The new guarded Settings backlight control was tested with off→on transactions. Complete original settings, both keymaps and Ripple parameters matched after restoration. Backlighting is left enabled, as requested.
