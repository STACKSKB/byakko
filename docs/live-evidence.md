# Hardware validation scope

Software tests cover protocol bytes, selected-target identity, pacing, backup order, readback, stale completions and failure retention. Bounded hardware observations cover several configuration reads and feature writes, but they do not establish full recovery or cross-platform acceptance.

Keep raw backups, device configuration, capture timestamps, screenshots and machine-specific observations private. Public technical findings belong in [engineering status](engineering-status.md), [parity status](parity-status.md), [protocol notes](settings-protocol.md), and [browser limits](webhid-parity.md).

Fault recovery, power-cycle persistence, prolonged host lighting, browser/OS combinations and notification permissions remain separate acceptance gates. No synthetic test or successful transport call proves physical output.
