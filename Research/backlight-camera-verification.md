# Backlight enable and camera verification

On 2026-09-22 the user requested that the backlight be enabled and explicitly authorized a webcam pointed at the keyboard. After USB replug, native reads succeeded. Options reply `0x86` held flags `0x10` and power-save `1`.

The supplied current bundle's option setter uses opcode `0x06`, profile at byte 1, flags at byte 2, Fn-matrix at byte 3, and power-save at byte 4 with BIT7 checksum. Its lighting panel disables controls when either the LED-off bit (bit 4) or power-save field is set. The original diagnostic `examples/enable_backlight.rs` backed up the complete settings, cleared only that LED-off bit and power-save, and checked the complete settings (excluding the option checksum byte when comparing the intended reply), both keymaps, and original lighting effect. Result: flags `0x00`, power-save `0`, other fields unchanged. These settings were intentionally left enabled as requested.

The saved effect was Ripple (5); its idle image did not show obvious light. `examples/lighting_camera_check.rs` temporarily selected steady light (1), brightness 4, red `[255,0,0]`, then green `[0,255,0]`. Native readback passed. Camera images visibly showed red then green/cyan illumination between and around the opaque keycaps. Camera color rendering is not a colorimetric measurement. Original Ripple settings were restored and compared; both keymaps and scalar settings remained unchanged. No physical key press or reactive-animation playback was tested.

Local ignored captures:

- `captures/keyboard-camera-1790050215559077500.png`: idle Ripple after enabling.
- `captures/keyboard-camera-1790050275805939100.png`: steady red.
- `captures/keyboard-camera-1790050282841905500.png`: steady green.

`capture_webcam.py` takes one still using the authorized camera, writes a new timestamped PNG, and releases the camera. OpenCV and NumPy were installed under ignored `Research/extracted/camera-python` solely for this research check; they are not product dependencies. No audio is recorded.

Host-stream frame encoders are separately implemented and unit tested, but official final USB capture and native screen/audio streaming remain pending.
