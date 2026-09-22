# Iced keymap migration acceptance

The desktop depends on core, devices and Iced. There is no dependency on the
legacy GUI package. The root research commands retain compatibility re-exports
to the same firmware implementation; no wire code was redesigned during the
move. The 22 implementation modules were compared against `f723d8b` after
namespace and rustfmt normalization; the adapter's conversion/test bodies were
also reviewed unchanged. Existing tests were relocated, not duplicated.

## Read-only native path, 2026-09-22

```console
cargo run -p byakko-devices --features research-tools --example read_keymap -- Research/captures/keymap-executor-after-migration.json
```

The probe reserves a new output file, creates the Nia87 descriptor and core
session, publishes the generation, submits only `Command::Read` through the
serialized executor and feeds the correlated completion back to core. It exits
successfully only when the core reaches Ready. No setters or keyboard edits are
in this code path. The full serialized completion is retained locally at the
path above, SHA-256:
`6eca5479044cdb2f8a79f3a39aa644c5e13b6d38b88b6c26b89fd17958db8190`.

The decoded revision snapshot equals the complete `keymaps` section of
`Research/captures/configuration-getter-trace-baseline.json`: format 1,
firmware 0x0100, profile 0, all 128 base and 128 Fn records including padding.
This confirms successful repeated device reads, backend translation, executor
delivery and core validation after the move. It does not reread other archive
sections or establish their current state.

## Remaining acceptance

- Iced mouse/keyboard interaction and rendered layout: screenshot helper still
  fails with `SetIsBorderRequired` / `0x80004002`; no accessibility expansion is
  being pursued. Window launch/normal close and resource observations are in
  `performance-baseline.md`.
- Write/readback/restoration through the new executor: existing transaction
  source and prior hardware evidence remain relevant, but this specific new
  path has only been exercised read-only on hardware.
- Physical key output, macro playback, persistence and real Linux hardware
  behavior still need their respective acceptance checks.
- The earlier whole-archive injected-failure recovery problem is unresolved;
  relocating the implementation does not repair or reaccept it.
- Iced macro files/labels, lighting/settings/archive surfaces, discovery/hotplug lifecycle
  and the remaining official configurator parity are still migration work.
