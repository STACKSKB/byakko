Byakko Keyboard Tester for WordPress
====================================

Install byakko-keyboard-tester.zip with Plugins > Add New > Upload Plugin, then
activate it. Create a separate public page, such as /keyboard-tester/, and put
this in a Shortcode block:

    [byakko_keyboard_tester]

The plugin renders the tester's heading and instructions in the page HTML.
Its small CSS and JavaScript files load only when that page contains the
shortcode. The stylesheet is queued before the page head is printed. The
shortcode must be in the page's saved content, such as a Shortcode block;
late-only template output cannot be detected before the page head. The
tester uses ordinary browser keyboard events, without device permissions,
an account, a server API, or telemetry. It does not transmit keystrokes.

Choose a physical layout and printed legend set in the tester. The initial
choices cover ANSI, ISO and JIS; full-size, TKL, 75%, 65% and 60%; and
QWERTY, QWERTZ, AZERTY and Japanese legends. Some compact
layouts are representative because manufacturers place a few keys differently.
The tester reports what reaches the browser; Fn and system-reserved shortcuts
may not produce browser events. A key that does not light up is not by itself
proof of a faulty keyboard.

index.html is the same self-contained tester for standalone hosting. The
source/ directory contains the corresponding editable source and build files.
The plugin and Byakko-owned assets are GPL-3.0-or-later; see LICENSE.

Build from repository source: cd web && npm ci && npm run build:tester:wordpress
Build from this ZIP's editable source: cd source/web && npm ci && npm run build:tester:wordpress
Run the independent tester checks with npm run test:tester (PHP is required
for the WordPress wrapper check).
Requirements for the build machine: Node.js, npm, Python 3. WordPress needs
none of these tools at runtime.
