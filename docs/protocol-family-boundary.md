# Protocol family and board boundary

The Nia87 is one observed Rongyuan-family board. Other Rongyuan PCBs may share
its HID framing and operations, but that is a hypothesis until a second board
is measured. We will extract the verified wire mechanics into a Rongyuan driver
and supply the Nia87's identity, layout and capabilities as data. A new board
profile must state its observed differences; it must not inherit Nia87 setter
support merely because its VID or GUI looks familiar.

The portable boundary remains `byakko-core`'s descriptors, drafts, commands and
typed outcomes plus `byakko-devices::Device`. Iced renders capabilities and
never branches on Rongyuan, Nia87 or VIA opcodes. A later QMK/VIA backend will
implement the same portable contract independently of the Rongyuan driver.
Opaque native values remain scoped to their backend, and native archives do
not silently become portable profiles.

The Rongyuan driver owns feature-report framing, selected HID collection
access, family codecs, read stabilization, and the ordered effect transactions.
Each feature module owns its complete read → expected-state check → durable
backup → setter → readback → recovery sequence. Shared helpers may own the OS
lock, target selection, exact same-handle guard, backup file mechanics and
typed recovery envelope. They must not hide feature-specific write order or
turn uncertain recovery into success. The Nia87 profile owns only observed
data: collection matcher, firmware/profile constraints, matrix geometry and
reserved slots, physical layout, action/effect catalogs, and supported
capabilities. Its `device.rs` should be a compact profile/facade, with no HID
transaction bodies or `read_macro_on_device`-style sequencing.

This is a small, typed driver API, not a report-language interpreter. Values
that are not proven variable remain named constants near their codec. We will
first preserve Nia87 output and safety tests while moving behavior. A second
Rongyuan board can then confirm which operations and profile fields are truly
shared. QMK/VIA support follows Nia87 through its own protocol adapter and
capability catalog; it does not emulate Rongyuan packets.
