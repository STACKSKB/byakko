# Macro workflow review — 2026-09-25

The user reported a permanently disabled Assign control and excessive movement
between editing and playback/assignment controls. The screenshot showed no
playback choice selected and unsaved recorded events. The desktop required an
explicit playback choice; Read did not select one. Macro binding also correctly
required saved, verified contents, but the UI did not explain that restriction
until a playback mode had been selected.

## Official-app profiling: partial evidence

The Windows agent handled `WIN-20260925-005` revision 1 against Byakko `9744f0f`.
The available executable identified itself as Nia87 Driver, ProductVersion
2.1.97.0 / FileVersion 2.1.97. Its launch failed because Windows denied desktop
access (`GetCursorPos`, 0x80070005). No current UI screenshots, control actions,
local draft edits or keyboard writes were obtained. Live workflow profiling
is deferred: the user cannot RDP into Windows and will restore access later.

Existing evidence supports keyboard/mouse-button recording, manual movement,
optional fixed delay, counted repeats 1–65,535, and distinct slot contents and
key-binding modes. Saving device macro contents and assigning bindings are
hardware effects. Earlier static evidence describes Clear all as also saving;
it must not be treated as a harmless draft action during future observation.
See [recorder timing](macro-recorder-timing.md),
[parity follow-up](macro-parity-followup.md) and
[boundary audit](macro-boundary-audit.md).

The retained official header capture (`Research/captures/macro-official-headers-1.log`
and `macro-official-commands-1.txt`, Windows-local ignored files) includes one
slot-0 macro setter. It does not establish a complete visual workflow or physical
playback. Create/select/name order, playback defaults, disabled-state messages,
save/assign UI coupling and the effect of selecting a key while editing remain
unobserved. Do not infer UI button structure from distinct protocol data.

## Approved Byakko direction

The user chose Save & assign together. Keep the board visible and group the
playback mode, repeat count, target key and save/assignment actions. Select a
compatible advertised playback mode by default and explain any remaining
restriction. Save and verify the macro before assigning its captured target
key; a failed macro save must never initiate the key assignment. Keep Save only
for library macros. These are Byakko design choices, not copied or observed
vendor UI behavior.
