# Focused macro recording

Load a macro slot before recording. Start recording to append to the local
draft, then keep the capture pad focused. Recording does not write the keyboard;
review the event stream, save the macro, and bind a key separately.

The capture pad accepts keyboard transitions and five mouse buttons: left,
right, middle, back and forward. Mouse clicks outside the pad stop recording
instead of adding interactions with other controls. Stop and focus loss append
release events for held keys and buttons. Space for those releases is reserved
within the 248-byte encoded limit. Repeated down events are ignored.

This recorder is local to the native window; it does not hook global desktop
input. Pointer movement and wheel actions can be edited manually but are not
recorded automatically. Event delays use frame timestamps rounded to
milliseconds; events within one frame share a timestamp. Keypad and left/right
modifier identity may be unavailable from the window event stream.

Headless event tests exercise the recorder and codec without sending input to
other applications or writing the device. They do not establish firmware
playback timing, movement semantics or physical key activation; those remain
hardware validation requirements.
