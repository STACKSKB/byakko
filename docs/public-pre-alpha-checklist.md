# Pre-alpha validation checklist

- Build the native workspace and optional browser artifacts from matching source.
- Run workspace tests, formatting, strict Clippy and browser tests.
- Review actual rendered layouts and control messages.
- Keep bounded hardware evidence separate from synthetic tests and transport acceptance.
- Record open recovery, persistence, platform and physical-output gaps.
- Audit the publication tree and history for secrets, raw user/device data, task logs and deployment details. Keep these private.
- Preserve GPL and third-party notices and reproducible build instructions.

See [source build](source-build.md), [Linux installation](linux-install.md), [engineering status](engineering-status.md) and [browser integration](webhid-browser.md).
