# Music Follow inspection, 2026-09-27

Read-only inspection opened the existing official Nia87 Driver 2.1.97 and
the already-authorized local renderer/helper research setup documented in
`rgb-white-boundary-20260927.md`. Native screenshot capture again failed with
`SetIsBorderRequired failed: No such interface supported (0x80004002)`.
The local browser renderer detected Nia87 USB. Its Light Mode dropdown visibly
offered Music Follow2 and Light Shadow, but neither Music Follow1 nor Follow3.
No music selection, Confirm, Reset or other keyboard setter was invoked.
The temporary tab, research server and RPC proxy were closed after inspection.

The selected Nia87 layout advertises native effect 20 as LightMusicFollow3,
21 as LightScreenColor and 22 as LightMusicFollow2. There is no Follow1 in this
board catalog. The shared renderer filters LightMusicFollow and
LightMusicFollow3 out of its non-Electron browser catalog; its Windows Electron
branch retains the selected layout. Thus the missing 1 is the vendor's board
naming, while the browser's missing 3 is a platform filter. This inspection
does not establish that the native Windows dropdown actually renders Follow3.
Byakko preserves its evidenced native 20/22 choices rather than inventing mode1.

Both music 2 and 3 consume the same six-row spectrum data in the shared
renderer. The official analyser uses a 2048-point FFT, magnitude smoothing
coefficient 0.4 and a 30 ms sampling timer. It takes byte frequency bins 27–58,
after subtracting the minimum of bins 20–39 and normalizing by the maximum of
the first 100 adjusted bins. At 48 kHz the displayed bins are approximately
633–1359 Hz. This is static algorithm evidence, not measured USB cadence or
physical output/playback evidence. No vendor source was copied into Byakko.

Byakko's bounded flicker correction now smooths magnitude before converting
to six discrete rows, using the observed 0.4 coefficient. Previously its
instant attack and fixed 0.7-row release quantized each new window abruptly.
Its nonblocking sampler now allows 90 ms without a ready packet before
injecting silence, because an empty drain is not an observed silent packet.
Actual silent packets still enter the analyzer immediately. Stop/close behavior
is unaffected. Tests cover a silent window retaining a fading tone and eventual
darkness, in addition to tone placement and invalid input.

Byakko retains its original logarithmic 60 Hz–12 kHz probe bank and Hann window.
Consequently this is not an exact official spectrum reproduction. A narrower
official-bin visualization and real sustained music comparison remain open;
physical flicker improvement cannot be concluded from unit tests. Settings
changes during active playback are implemented separately by the host session
owner. Selecting official music to inspect its active controls was deliberately
omitted because it would start keyboard writes without physical-test permission.
