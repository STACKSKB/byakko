# Protocol family and board boundary

The Nia87's own official-app captures and read-only USB replies use a
`yc500`-shaped command set: `0x85` reads the active profile, `0x89` reads a
matrix page, and `0x91`/`0x92`/`0x86` read debounce, sleep and options.
Its selected official-app code uses `0x13`/`0x15` for individual key writes.
These observations establish the Nia87 path implemented here. They do not
establish that every Rongyuan PCB accepts those writes.

[Sharkfin's independently published protocol notes](https://github.com/dniminenn/sharkfin/blob/master/docs/PROTOCOL.md)
describe at least two command families, `yc500` and `gen2`. Several opcode
numbers collide while their meaning or payload shape differs. In particular,
a `yc500` matrix write opcode is a `gen2` options write opcode. The notes also
describe boards within one family that lack the individual-key setter. Their
macro notes suggest a shared page shape with different family write opcodes;
we keep our present codec scoped to the captured Nia87/`yc500` path until a
second board can validate a wider interface. A shared lighting opcode likewise
does not imply identical speed interpretation or effect availability. This
document uses those findings to draw boundaries; implementation and test
vectors must come from our own captures or independently constructed cases.
No Sharkfin source or UX is incorporated.

| Owner | Responsibility |
| --- | --- |
| `byakko-core` | Device-neutral capabilities, drafts, commands, transitions and typed outcomes. |
| `byakko-devices::rongyuan::report` | Shared, observed 64-byte framing and checksum mechanics. No write opcode selection. |
| `byakko-devices::rongyuan::yc500` | `yc500`-shaped matrix and macro codecs plus the observed macro read sequence. Further ordered feature transactions move here when verified. `gen2` will have a distinct namespace and codec. |
| Nia87 profile | Collection identity, accepted firmware/profile, matrix dimensions and reserved slots, physical layout, action and effect catalogs, and capabilities proven on this board. |
| HID/session infrastructure | Target-pinned open, lock, pacing, backup mechanics and typed recovery envelope; it does not choose feature-specific write order. |
| QMK/VIA backend | Independent protocol adapter implementing the portable device contract; it never emulates Rongyuan reports. |

A board profile must identify its command family and supported write strategy
before a setter can be constructed. A USB VID/PID or a common reply opcode alone
is insufficient. Discovery and classification remain read-only. Unknown boards,
firmware revisions and unsupported setter variants may be displayed as
unconfigured or read-only; they cannot inherit Nia87 write capability. A
`gen2` device must never be passed to a `yc500` writer even though both may
use the same HID collection and checksum. The physical key-to-slot mapping is
board data; the command layout is family behavior.

The OS HID inventory is device-neutral. A read-only smoke run on 2026-09-23
on this Windows host found eleven Wacom Intuos Pro collections and seven from the
Nia OEM device through the same enumerator and exact-target selector. The
Nia87 adapter then selected only its `FFFF:0002` configuration collection.
Opening a feature-report handle still performs Nia87-specific identity and
report-length checks. This external-target check proves the discovery boundary,
not tablet button/ring decoding or generic write support.

A future Wacom adapter can reuse collection inventory, exact target selection,
session tickets, draft transitions and capability-rendered controls. Its tablet
reports and supported button/ring actions need their own evidence and codec.
Button-like ring steps may fit the current binding descriptor; continuous ring
values do not yet have a portable domain type. Add that type when a tablet
backend provides an observed contract, rather than copying Nia87's mouse/wheel
report bytes or treating every HID collection as a keyboard. The synthetic
tablet contract test exercises ExpressKey and discrete ring bindings through
the same core draft and memory-device apply path, without a tablet driver.

Within each feature, keep the ordered read -> expected-state check -> durable
backup -> write -> complete readback -> recovery sequence explicit. Shared
helpers can supply a session and typed failure envelope. They must not hide
which reports are written, invent a fallback write strategy, or report
uncertain recovery as success. Move feature transactions from Nia87 into the
family driver only after their shape and safety rules are supported by
independent evidence; do not turn the profile into a report-language
interpreter. Geometry and capability values can be data while protocol control
flow stays small, typed code.

The Iced desktop composes views from portable capabilities and projections; it
does not branch on Rongyuan, Nia87 or VIA opcodes. A future browser frontend
may consume the same core model or call a native service adapter. Native
archives remain backend-specific, distinct from portable profiles. QMK/VIA
support follows Nia87 through the device contract rather than sharing this
OEM's packet representation.

USB cable and 2.4 GHz receiver access are transport variants, not command
families. The receiver's settings relay must be discovered and validated on the
connected hardware; a matching board name cannot imply that its receiver
forwards reads or safely accepts writes. USB remains the only supported Nia87
configuration transport until those observations exist.
