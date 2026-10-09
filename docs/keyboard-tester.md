# Standalone keyboard tester

The keyboard tester is a separate browser page from the Byakko configurator.
It uses ordinary keyboard events, so it does not request WebHID permission,
connect to a device, or change keyboard settings. Its WordPress shortcode
renders useful page text as HTML before JavaScript runs. WordPress loads the
tester stylesheet and script only on a page containing that shortcode.

The tester offers common physical presets: full-size, TKL, 75%, 65%, and 60%
boards, with ANSI, ISO, and JIS variants. Printed legend choices include
QWERTY, QWERTZ, AZERTY, and Japanese JIS legends. Selecting JIS geometry
also selects the Japanese legends initially. Compact boards do not have one
universal physical arrangement; their geometry is representative. The physical
key highlight follows `KeyboardEvent.code`, while the event details show `KeyboardEvent.key`
as delivered by the visitor's active operating-system layout. Selecting a
printed legend changes the illustration, not the operating-system layout.

The tester shows pressed and previously tested keys, a tested-key count,
simultaneous input, recent event details, and a reset control. It responds
automatically while the page is focused; ordinary form fields remain usable
and their text input is ignored. If a page contains multiple testers, the first
receives input initially; clicking or focusing another makes it active. Reset
test is at the top right and clears the active test immediately.

Tab registers as a test key while capture is active. Shift+Tab stops capture,
releases held keys and moves focus backward for ordinary page navigation.
Navigation remains available when focus returns to the tester or the page.
Click the keyboard, or focus it and press Enter or Space, to resume testing;
the activation key does not count as a test sample. Reset test also resumes
capture. Browser and operating-system shortcuts can be
reserved, and Fn usually has no independent browser event. The tester can
show which events arrived in the browser; it cannot prove that a missing event
means a hardware fault. Keystrokes stay in the browser and are not sent to a
server or stored for later visits.

## WordPress installation

Build `web/dist/wordpress-tester/byakko-keyboard-tester.zip`, upload it under
**Plugins → Add New → Upload Plugin**, and activate it. Create a dedicated page
such as `/keyboard-tester/` and insert a Shortcode block containing:

```text
[byakko_keyboard_tester]
```

Give the WordPress page its own title, description, and permalink. The tester
heading and instructions are in the shortcode's HTML, so the page is useful
to readers and search engines before its script runs. The tester CSS is scoped
to `.byakko-tester`; it can appear more than once on a page. The plugin does
not change the separate Byakko configurator shortcode.
Place the shortcode in the page's saved content (a Shortcode block works);
that lets WordPress detect it before printing the page head and load the CSS
there. A shortcode emitted only by a late theme template or page builder is
outside this loading path.

`index.html` inside the plugin ZIP is also a self-contained standalone build.
It can be hosted as a static HTML file without WordPress or a Node.js server.
The ZIP contains the editable source under `source/` and the GPL license.

## Reproducible local build

From the repository root:

```sh
cd web
npm ci
npm run build:tester:wordpress
npm run test:tester
```

The build machine needs Node.js, npm, and Python 3 (for the reproducible ZIP).
If Python 3 is installed under another command or path, set the `PYTHON`
environment variable to that executable before building or running tests.
The WordPress server needs only PHP and the uploaded files. To check the
WordPress plugin behavior locally, `php` must be available on the test machine.
No device is required for the build or tests.
The optional browser interaction checks use an installed Chromium executable:

```sh
npm run build:tester
CHROMIUM_BIN=/usr/bin/chromium npm run test:tester:browser
```

Set `CHROMIUM_BIN` to the executable's location on your machine. Browser tests
are skipped when this variable is absent; the model, build and WordPress checks
run independently of Chromium and the configurator's WASM build.

The tester reuses the browser configurator's key geometry functions and physical
code-to-USB-usage mapping. Its declarative layout catalog and input state are
independent of device sessions. Both the standalone and WordPress builds use
the same tester source. The build rejects JavaScript and CSS exceeding a combined
30 KiB gzip budget. No framework, WASM, font download or background polling is
included in the tester.

The "most held at once" display counts the keydown events that reached this
page during the current test. It does not certify a keyboard's rollover limit.
