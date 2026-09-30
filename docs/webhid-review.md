# WebHID review and reading map

Reviewed 2026-09-30: `0be1cd9..a4c07b0`, the five WebHID/design commits.
This is a source review, including independent browser and Rust reviews, with
fresh workspace tests, browser tests and strict Clippy. It is not additional
physical acceptance. Findings below describe the reviewed revision. All three
correctness findings are now fixed and covered by eight real-DOM/WASM tests.
The original findings below are historical and fixed. A follow-up review of the
browser refactor on 2026-10-01 confirmed the additional fixes recorded in
the next section. All 23 real-DOM/WASM cases pass. No Rust changes or native
dependency differences were found.

## Findings

### P2: a macro import can land in a different slot

Fixed: import acceptance checks the captured device, generation and slot before
staging and accepting metadata. Stale failures are ignored as well.

In [app.mjs](../web/app.mjs), the macro file change handler awaits `file.text()`
before dispatching `importMacroDocument` (reviewed lines 1144–1154). The dispatch
uses the currently selected core slot, but the name and binding metadata use
the slot captured when the control was rendered. Select slot A, start an import,
then select B while the read is pending: the document can be staged in B while
its metadata is associated with A. A reconnect during the read has the same
missing identity check. This stages a draft; it does not itself write hardware.

Capture device, connection generation and slot at import start and reject the
late result if any changed before dispatch. Test with a deferred file read and
both slot selection and reconnect through the real UI handlers.

### P2: completing a name save can discard newer text

Fixed: completion checks the captured device/generation and clears only a draft
that still equals the submitted name. Stale failures cannot overwrite notices.

`saveMacroName` in [app.mjs](../web/app.mjs) (reviewed lines 459–470) awaits
IndexedDB and then unconditionally deletes the local name draft. If a user
types another name while that save is pending, completion erases the newer
draft. If they disconnect/reconnect, completion can also update the new session's
name cache with the old session's result. The durable write targets the original
product; the problem is acceptance into the current UI.

Accept the result only for the captured connection generation, and clear a draft
only when it still equals the submitted name. Test a deferred store completion
with newer typing and with reconnect.

### P2: notifications can discard a focused form edit

Fixed: notifications and asynchronous command completions defer form redraws
while an input, select or textarea has focus. The real control and unfinished
value stay intact, so the observation gate still sees editing. Status/connection
feedback can update independently. The deferred render flushes on leaving the
form; committing an edit or explicit navigation still renders immediately.

`handle` calls `render` before `scheduleObservation` in
[app.mjs](../web/app.mjs) (reviewed lines 198–200). `render` replaces the entire
feature panel (1174–1184), removing the focused control. The observation gate
(211–226) then checks focus after it has been lost. An onboard event during a
numeric edit can discard a value that has not yet emitted `change`, and start
the background read that the focus gate was intended to defer. Other asynchronous
completions also rebuild controls and can interrupt editing.

Preserve active controls and unfinished form values across background updates;
evaluate editing activity before replacing DOM. Test notification delivery while
a numeric field contains an unfinished edit, then verify focus, value and deferred
read behavior. Existing transport/WASM tests do not cover this DOM interaction.

## Follow-up review: current browser refactor

The follow-up confirmed several intent-retention and lifecycle fixes:

- A macro import captures device, connection generation, slot, import request
  version, core draft and local form revision. Late reads cannot replace a newer
  core edit, local name/event-form edit, focused unfinished field, newer import,
  changed slot or reconnected session. The guarded import path is in
  [macro-form.mjs](../web/macro-form.mjs).
- Leaving Macros stops recording. Recorder stop releases held keyboard inputs
  through core; recorder reset clears local held pointer state and invalidates an
  in-flight start. Disconnect also resets the recorder, preventing old pointer
  releases from reaching a later recording. See [recorder.mjs](../web/recorder.mjs)
  and the disconnect callback in [app.mjs](../web/app.mjs).
- Local macro names are scoped by product and retained as drafts across
  disconnect/reconnect. An older save completion cannot clear newer text or
  update the new session. `beforeunload` includes unsaved local names. Storage
  load preserves saves that complete while loading. See
  [macro-form.mjs](../web/macro-form.mjs) and [storage.mjs](../web/storage.mjs).
- Escape, native close and explicit buttons all settle the discard dialog's
  pending action. Slot switches and reverts also retain their original connection
  and slot while waiting for that decision. See [discard-dialog.mjs](../web/discard-dialog.mjs).
- Runtime notices retain typed error severity during background notifications;
  rendering no longer infers severity from message text. See
  [runtime.mjs](../web/runtime.mjs).

An initial-name-load race was considered for an adapter design, but the current
IndexedDB request ordering does not establish a production bug; do not report it
as a finding. These changes add no Rust edits and do not alter native dependency
boundaries.

## Structure and reading order

| Owner | Start here | Responsibility |
| --- | --- | --- |
| Portable application | [core/session.rs](../crates/byakko-core/src/session.rs), `editor/`, `workflow/` | Baselines/drafts, validation, connection/operation correlation, save-and-assign and host lifecycle. Shared by native and browser. |
| Portable Nia87 rules | [protocol/nia87/mod.rs](../crates/byakko-protocol/src/nia87/mod.rs), `board.rs`, `protocol.rs`, feature adapters | Board capabilities, report codecs, exact raw bytes and conversion to/from core values. No HID or storage. |
| Browser Rust boundary | [web/session.rs](../crates/byakko-web/src/session.rs) | `BrowserSession`: JSON intents into core, core outcomes and projected views back to JavaScript. |
| Browser transaction planner | [web/executor.rs](../crates/byakko-web/src/executor.rs) | `BrowserOperation`: pure backup/exchange/write/completion steps, pacing and recovery plans. |
| Browser host planner | [web/host.rs](../crates/byakko-web/src/host.rs) | Host startup, frames, updates and restoration using shared codecs and core tickets. |
| Browser effects | [executor.mjs](../web/executor.mjs), [transport.mjs](../web/transport.mjs), [storage.mjs](../web/storage.mjs) | One selected-device queue, exact report boundary, WebHID calls and committed IndexedDB backups. |
| Browser notification/capture effects | [notifications.mjs](../web/notifications.mjs), [host.mjs](../web/host.mjs), [sampler.mjs](../web/sampler.mjs) | Selected input interface, capture lifecycle, bounded latest-frame queue, screen/shared-audio samples. |
| Browser composition and runtime | [main.mjs](../web/main.mjs), [app.mjs](../web/app.mjs), [runtime.mjs](../web/runtime.mjs) | Startup, navigation/render composition, focus preservation, notice severity, connection and ordered browser effects. `app.mjs` is now about 200 lines. |
| Browser feature forms | [keymap-form.mjs](../web/keymap-form.mjs), [settings-form.mjs](../web/settings-form.mjs), [macro-form.mjs](../web/macro-form.mjs), [lighting-form.mjs](../web/lighting-form.mjs), [diagnostics-form.mjs](../web/diagnostics-form.mjs) | Feature controls and browser-local form state; core owns portable editor values and domain rules. |
| Browser keyboard and recording | [keyboard.mjs](../web/keyboard.mjs), [recorder.mjs](../web/recorder.mjs), [host-form.mjs](../web/host-form.mjs), [input.mjs](../web/input.mjs) | Keyboard projection, independent local recording with held-input release, and host capture controls/parameter drafts. |
| Browser backup and JSON review | [backups.mjs](../web/backups.mjs), [json-dialog.mjs](../web/json-dialog.mjs), [storage.mjs](../web/storage.mjs) | Committed diagnostic backups, durable local-name storage and exact JSON review/export. |
| Browser presentation details | `*_view.mjs`, [widgets.mjs](../web/widgets.mjs), [style.css](../web/style.css) | Feature projections, reusable DOM controls and browser visual design. |
| Local static packaging | [build-static.mjs](../web/build-static.mjs), [license-inventory.mjs](../web/license-inventory.mjs), [build.ps1](../web/build.ps1) | Build-time embedding of JS, WASM, CSS, logo and license notices into `web/dist/index.html`. Node and Rust run only on the build machine; the server serves this one HTML file. |
| Optional WordPress packaging | [build-wordpress.mjs](../web/build-wordpress.mjs), [byakko-configurator.php](../web/wordpress/byakko-configurator.php), [embed.js](../web/wordpress/embed.js) | Packages the exact static client. PHP returns `[byakko_configurator]`; the static parent script fits the iframe to its content and positions dialogs within the visible page. Embedded styling uses a neutral palette, parent font and separate appearance preferences. These owners perform no keyboard operations. |
| Native effects/presentation | `byakko-devices`, `byakko-desktop`, `byakko-cli` | Continue to use native HID, native storage and Iced/CLI. No browser runtime dependency. |

A normal save follows this path:

```text
DOM edit → BrowserSession → core editor draft
Save → core Command → BrowserOperation
     → DeviceExecutor → committed backup → paced WebHID effects
     → Completion → core acceptance → projected view → DOM
```

Keymap, settings and macro writes verify one readback. Ordinary lighting/picture
writes finish on transport acceptance after pacing, without an automatic getter.
Notifications enter shared `core/workflow/observation.rs`; they are coalesced
into reads of affected loaded features rather than periodic feature polling.
Host frames and restoration use the same selected-device effect queue.

## What changed in native code

The large native diff primarily extracts pure Nia87/Rongyuan code from
`byakko-devices` into `byakko-protocol`; native adapters now retain their I/O and
call the extracted rules. Audio analysis moved unchanged into
`core/projection/audio.rs`. The desktop observation scheduler moved into
`core/workflow/observation.rs`, accepting elapsed milliseconds instead of
`Instant`; desktop supplies its monotonic clock. Browser JavaScript does not
enter the native dependency tree. This is one branch supporting both products.

The review found no supported Rust/native regression. That is a review result,
not a guarantee of unchanged physical behavior on every platform. Browser and
native ordered transactions are still separate implementations around shared
rules, so changes to pacing/recovery need checks on both paths.

## Maintainability and evidence limits

The original three fixes have eight real-DOM/WASM regression cases in
`web/app-tests.html`; follow-up intent and lifecycle fixes are also represented
there. Serve `web/`, open that page and choose Run tests. It uses simulated HID
and deferred file/storage effects; it cannot request physical keyboard access.

The previous 1,290-line `app.mjs` concern is resolved by composition into
feature forms and lifecycle owners. Keep ownership explicit: portable editor
values and domain decisions belong to core; browser-only unfinished controls,
local names and recording activity belong to their local owners. Forms consume
core projections and call explicit runtime operations; they do not share a bag
of mutable application state.

Current checks passed: 483 workspace Rust tests (one hardware test ignored),
44 Node tests, 23 real-DOM/WASM cases, Clippy with warnings denied, formatting,
native desktop/CLI builds and the release WASM build. The DOM cases exercise
actual remap, macro save-and-assign, all 23 lighting selections, settings,
host start/stop restoration reports and diagnostic controls as well as the
intent/lifecycle regressions. Rendered light/dark layouts were checked at
1280×800, 1024×768 and 390×844; the narrow keyboard scrolls without page overflow.

Physical settings/host output, injected recovery failures,
power-cycle persistence, standalone Chrome/Edge, PID 4011 and Linux WebHID
remain acceptance gaps; see [browser status](webhid-browser.md) and
[parity matrix](webhid-parity.md). Limited physical remap/RGB/macro restoration
checks cover only their tested operations.
