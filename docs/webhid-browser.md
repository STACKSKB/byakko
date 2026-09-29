# WebHID browser integration

The optional browser client reads and edits Nia87 keymaps, global RGB, per-key
RGB pictures and macros, including macro recording and save-and-assign. It also
supports settings, notification-driven onboard refresh, screen/music host
lighting and native-format diagnostic capture/export. The native Iced product
and its build require no JavaScript, browser, Node.js or server.

## Build and deploy the static page

The deployable application is **`web/dist/index.html`**, one self-contained
static file. It embeds the JavaScript, WASM, CSS, logo and license notices.
The browser loads that file and runs configuration, HID operations, recording,
capture and IndexedDB storage locally. There is no API, application server,
Node.js process or native bridge on the host. Node runs only on the developer's
machine for build tooling and tests; none of its packages are deployed.

Install the Rust `wasm32-unknown-unknown` target, `wasm-bindgen-cli` **0.2.128**
(matching Cargo.lock) and Node.js for local builds. The generated files in
`web/pkg`, build dependencies in `web/node_modules` and `web/dist` are ignored.

```powershell
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128 --locked
npm ci --prefix web
./web/build.ps1
python -m http.server 8765 --bind 127.0.0.1 --directory web/dist
```

`build.ps1 -WasmBindgen <path-to-wasm-bindgen>` accepts a separately installed
official binary. On Linux the equivalent build is:

```sh
npm ci --prefix web
cargo build --locked -p byakko-web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/pkg --out-name byakko_web \
  target/wasm32-unknown-unknown/release/byakko_web.wasm
npm --prefix web run build:static
python3 -m http.server 8765 --bind 127.0.0.1 --directory web/dist
```

Copy **only `web/dist/index.html`** to any static HTTPS host. It needs ordinary
HTML serving; there are no separate JavaScript/WASM MIME settings or server
installation steps. The Python commands above are optional development
previews, serving static files only. No deployment is performed by the build.

Open `http://127.0.0.1:8765` for a local preview, or the deployed HTTPS URL,
in a WebHID-capable desktop browser. Close other configurators, choose the
keyboard in the browser permission flow, and use the keyboard workspace to
read and edit the supported features. Read controls are explicit; there is no
periodic feature polling. **Disconnect** invalidates in-flight work and closes
the selected device after current browser I/O settles.

The separate files in `web/` remain readable development sources. Developers
can serve that directory to work on source modules or use `web/app-tests.html`
for DOM tests, but it is not the deployment artifact. `npm --prefix web test`
runs local Node tests; they do not introduce a server runtime dependency.

## Optional WordPress page

The WordPress target packages the identical static client with a small PHP
shortcode wrapper. It keeps the configurator's CSS and document separate from
the WordPress theme, while displaying it inside a normal page. WordPress's
existing PHP renders the iframe; keyboard operations and storage still run in
the visitor's browser. There are no server-side configurator actions or APIs.

On Windows, build the installable plugin ZIP with:

```powershell
./web/build.ps1 -WordPress
```

The result is **`web/dist/byakko-configurator.zip`**. Upload it through WordPress
**Plugins → Add New → Upload Plugin**, activate it, and add a **Shortcode** block
to a page containing:

```text
[byakko_configurator]
```

A full-width, unboxed template with extra content
spacing disabled gives the keyboard room. The embed grows with its content,
so the WordPress page owns scrolling. Its small static `embed.js` measures the
same-origin document and positions dialogs within the visible part of the page.
The embed inherits the site's font family and defaults to the Neutral light
palette; its appearance preferences use a separate browser storage key. The
standalone layout and defaults remain intact. A link below the embed opens
the same static file in its standalone layout.
Updates replace the plugin package; its bundle URL includes a file modification
version so the page does not retain an older asset URL.

On Linux, after the static build above:

```sh
npm --prefix web run build:wordpress
cd web/dist/wordpress
zip -r ../byakko-configurator.zip byakko-configurator
```

Keep the plugin's HTML asset on the same HTTPS origin as the page, including
when using a CDN. The iframe allows `hid` and `display-capture`, whose default
permissions-policy allowlist is `self` ([WebHID](https://wicg.github.io/webhid/#permissions-policy),
[screen capture](https://www.w3.org/TR/screen-capture/#permissions-policy-integration)).
An existing site policy that disables either feature still applies. The wrapper
does not change security headers or request device permission automatically.
WordPress's [shortcode API](https://developer.wordpress.org/plugins/shortcodes/)
avoids putting the 2.7 MB application script into editable post content, which
WordPress can filter depending on [HTML permissions](https://wordpress.org/documentation/article/custom-html/).

Packaging checks verify that the ZIP contains the same static client. Local
DOM/WASM tests exercise the same-origin iframe, appearance controls, layout and
dialog positioning. A site's own permissions policy, theme and caching can
affect the embedded page, so verify HID permission, scrolling and dialog
visibility on the intended host before publishing it.

## Ownership and transport

- `byakko-core` owns browser editor lifecycles, baseline/draft/submitted state,
  validation, correlated commands/completions, macro recording and cross-feature
  workflows. `BrowserSession` in `byakko-web` projects this shared session to
  the browser and forwards user intent to it.
- `byakko-protocol` owns portable Nia87 board/action/layout rules, keymap,
  lighting, picture, macro, settings, archive and host projections, report codecs, and recovery slot
  selection used by both transports. Native callers import this owner directly.
  Raw revisions and the official semantic-white convention stay shared.
- `BrowserOperation` in `byakko-web` plans pure report steps, readback, pacing
  requirements and typed recovery from core commands. It does not access HID or
  storage. `web/executor.mjs` performs ordered WebHID effects;
  `web/storage.mjs` commits before-image backups to IndexedDB before setters.
- `web/transport.mjs` validates permission selection and the exact
  collection/report shape. `web/app.mjs` composes feature forms, independent
  recording and browser runtime effects; see the [reading map](webhid-review.md).
- `web/build-static.mjs` uses [esbuild's browser bundling](https://esbuild.github.io/api/#bundle)
  only at build time. Its binary/data loaders embed WASM and the logo, and its
  CSS bundle includes palettes. Packaging rejects external module outputs,
  imported stylesheets or emitted assets. The pinned generated WASM glue must
  match the known embedded-byte loader; a changed signature fails the build.
  `web/license-inventory.mjs` embeds GPL and third-party notices from the locked
  browser dependency graph. Neither build module enters the browser bundle.

The chooser filters VID `3151`, PID `4011`/`4015` to receive all interfaces of the selected keyboard. Configuration validation requires usage page `FFFF`, usage `2`. The notification listener uses only the same chooser result, vendor collection `FFFF:1`, input report 5 with exactly three payload bytes.
Selection requires exactly one matching configuration interface and one
unnumbered 512-bit feature report. Browser APIs do not expose the native device
path, interface number or physical USB ancestor, so the browser retains the
selected device object and never falls back to another granted device.

WebHID `sendFeatureReport(0, payload)` receives exactly 64 payload bytes, without
the native API's synthetic report-ID prefix. The existing 30 ms read interval
precedes `receiveFeatureReport(0)`, whose 64-byte DataView is checked. Keymap
and macro saves take one readback; ordinary lighting and picture saves finish
on transport acceptance after established pacing. Backups commit before the
first setter. No arbitrary opcode interface, padding, truncation or automatic
getter after an ordinary lighting/picture setter is introduced.

Disconnect invalidates in-flight results through core operation/generation
correlation. Device close waits for outstanding browser I/O to settle, preventing
a new session from overlapping the old one. A browser/driver call that never
settles can delay teardown; reload the page if it remains stuck. There is no
pretend cancellation via a timeout that would leave untracked I/O running.

WebHID requires browser permission and a secure context, and protects some HID
collections. The successful configuration-collection read below does not prove
that the separate onboard-notification collection is accessible. See the
[Chrome access guide](https://developer.chrome.com/docs/capabilities/hid) and
[WebHID specification](https://wicg.github.io/webhid/).

## Verification and acceptance boundary

Local Rust, Node and DOM/WASM tests exercise report validation, backup-before-write
order, pacing, readback and recovery, stale completions, appearance, and the
browser control paths. The static build tests verify embedded WASM, styles and
license notices and reject unintended external application assets. These tests
use simulated WebHID and storage effects.

Limited physical checks have covered an initial keyboard read, keymap remapping,
global and per-key lighting changes, a macro save-and-assign, onboard input
notifications and restoration of the tested values. This evidence applies only
to the tested Nia87 and operations. Settings writes, physical host lighting
output, fault recovery, power-cycle persistence, standalone browser variants,
PID 4011 and Linux WebHID still need acceptance. A simulated control test or
successful transport call does not establish visible hardware output.

The browser backup viewer offers read-only JSON and Download for recovery
inspection. Diagnostic exports can retain private keyboard settings or macros;
review them before sharing. Public archive restoration is not provided.

## Shared browser behavior

The implementation and remaining acceptance matrix is in
[webhid-parity.md](webhid-parity.md). The selected notification interface feeds
the shared core observation scheduler: 500 ms coalescing, two seconds for
reset/profile, affected loaded features only, queued activity and retained
conflicts. Listener failure exposes manual read/reconnect; it does not start
feature polling or re-enumeration.

Settings use shared capabilities and one-field editing with native pacing,
backups, readback and recovery. Macro names persist in IndexedDB; recording
supports fixed/measured delays and local pointer buttons. Diagnostics capture
the full native archive format for review/export, including after disconnect.

Host screen/music uses browser-selected capture with the shared Rust host
lifecycle, frame codecs and audio projection. Source preparation precedes
writes, frames are bounded, live parameters retain the original baseline, and
Stop waits for verified restoration. A failed restoration retains a connected
manual-read path. Focus loss does not intentionally stop capture; page freezing
or forced termination can prevent timely frames/restoration. Stop before closing
the tab. Shared audio availability depends on the selected source, browser and
operating system.

The build emits one self-contained `web/dist/index.html`. Readable development
sources remain separate in `web/`. Only the generated HTML is deployed; native
builds and the native product require no browser runtime.
