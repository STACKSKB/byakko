# Recorder timing and selected official behavior

Read-only reference audit on2026-09-22 used the current ignored web bundle
`Research/extracted/web-current/main_68eaf5ce.js`. No vendor implementation is
incorporated. Character offsets identify the inspected selected paths.

The official recorder `spn` near23,714,937 computes elapsed time since the
previous transition. The first transition contributes only an action; later
transitions contribute a delay followed by the new action. The selected
`YY.setMacro` encoder near15,390,887 pairs each action with its following delay.
Therefore, in Byakko's combined event model, the elapsed interval belongs to
the previous event, not the new event.

The previous native recorder put elapsed time on each newly appended event.
For example, starting at0ms, pressing at100ms, releasing at350ms produced
down(wait100), up(wait250). The intended transition spacing is instead
down(wait250), up(wait0): recording startup latency is omitted and the final
action has no added trailing wait. The same issue affected synthesized releases
on focus loss or Stop.

The corrected recorder tracks the last event in the current recording session.
It updates that event's wait only when the next transition arrives, then adds
the new event with zero trailing wait. Appending a recording preserves the
pre-existing draft's delays. Stopping with held keys assigns the final hold
interval before adding releases; stopping after all releases adds no tail for
time spent reaching the Stop button. Capacity checks include pending delays
and release events before committing changes to the draft.

Device-free tests exercise asymmetric timings, append isolation, long initial
idle and held-key release at Stop, alongside existing capacity/focus tests.
These establish recording/model behavior. They do not establish physical
firmware playback timing; that still requires activation of a bound macro.

## Remaining editing parity

The selected official hook near23,743,033 subscribes to keyboard and mouse-button
transitions, not pointer movement or wheel events. Movement is manually
insertable near25,181,460 and25,186,454. Its timeline exposes Clear all near
25,194,960, and a fixed recording-delay option near25,195,212 (1–65,535ms),
applied to incoming delay records near23,734,305. No distinct no-delay toggle,
bulk modification of existing delays, or event-duplication action was established.
Byakko's manual movement and focused keyboard/button recorder cover those
event types; fixed-delay recording and Clear all remain implementation gaps.

## Save-path follow-up

A later read-only audit of the same bundle found a further timing distinction.
Its SHA-256 is
`3708db6c84edce9ca8a2424d744a9ee8a1ef66a267f6f3eca019711d9bf3feca`.
No reference code is incorporated.

- The recording-delay default near23,732,673 is1ms. The UI bounds the fixed
  value to1–65,535ms. Incoming delay records use the configured fixed value when
  enabled (near23,734,305). The option affects new recording delays; its setters
  near23,740,871 do not rewrite existing timeline delays.
- `uiSaveMacro` near23,744,730 appends a delay to a nonempty timeline whose last
  record is an action:50ms normally, or the configured fixed value. An existing
  terminal delay remains intact. It also forces repeat count1 outside repeat-count
  mode. These are save-time rules beyond the transition-order correction above.
- The clear action near25,194,960 empties the timeline and saves the selected
  macro through `uiSaveMacro`. Persistence proceeds through the app macro-list
  helper near23,735,629. A native clear control should stage the empty draft and
  retain the application's explicit device-write boundary.

For a newly recorded press at100ms and release at250ms, the observed reference
pipeline therefore produces waits150ms and50ms after save. With a fixed10ms
setting it produces waits10ms and10ms. The native recorder currently produces
150ms and0ms in measured mode. The difference may matter between repetitions;
physical timing remains unverified. The earlier correction established elapsed
delay association, **not complete save-path timing parity**.

Future recorder parity work must account for terminal-delay policy explicitly,
preserve existing imported/manual delays (including explicit zero), and reserve
capacity for the selected policy. Do not silently rewrite existing macro files.
