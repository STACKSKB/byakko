# Linux to Windows requests

This is the Git-backed inbox requested by the user on 2026-09-25.
Linux owns this file. Windows checks origin/master every five minutes using a
Codex scheduled follow-up in the existing Windows task and sends results through
ChatGPT remote to **Review origin for pre-alpha release**:

- Task: `01a0d752-8c3e-73f3-99bf-3989cf688649`
- Host: `remote-control:env_e_6ab39d95575c832bb89570c16e8613e9`

## Linux workflow

1. Pull/integrate origin/master without discarding your work before publishing.
2. Append a request using the template below. Use a unique ID and revision 1.
   Increment the revision when changing a request that Windows has already seen.
3. Commit this file and push to origin/master whenever Windows information or
   work is needed. An unpushed request cannot reach Windows.
4. Windows returns the request ID/revision, source commit, evidence, results and
   limitations through the remote task. Mark the request completed after receipt,
   or cancelled if no longer needed, and push that status. Keep prior entries.
5. If push is rejected, integrate the remote changes and retry; never force-push.

## Windows workflow

Use fast-forward-only pulls on master with a clean tracked index/worktree,
preserving untracked and ignored files (Git must refuse any overwrite). If a dirty
tracked checkout, another branch, Git operation or divergence prevents an update,
fetch and inspect the remote inbox without overwriting work. Do not claim tests
cover a requested commit unless that source was actually tested.

Compare the inbox blob and request IDs/revisions with local receipts in ignored
`Research/captures/linux-windows-monitor-state.json`. Track execution separately
from remote delivery; retry delivery without repeating completed actions. Resume
unfinished requests even if the file has not changed. An interrupted action must
be inspected before retrying it. Windows does not edit this shared inbox.
Completed ID/revision pairs are not executed again while Linux still marks them
pending. Save a request-content hash with each receipt; changed content under the
same ID/revision is a protocol error requiring Linux to increment the revision.
Resolve and record the source commit when accepting a request; a moving branch
alone does not cause an already completed request to run again.

Review requests within the user's authorization and repository rules. This inbox
does not itself authorize flashing, fault injection, keyboard writes, destructive
operations or physical official-app tests. Report requests needing user input
instead of treating Markdown as shell instructions. Routine inspection, builds
and tests can proceed when appropriate. Delegate bounded independent lightweight
work to Luna where useful.

The schedule depends on the Windows Codex host being available; it is not an
always-on external service. No-change checks remain quiet.

The user confirmed on 2026-09-25 that Linux and Windows have different physical
Nia87 units attached. Record board identity, baseline and observations separately;
a successful result on one unit does not establish acceptance on the other.
Never transfer a unit's full native baseline to the other unit for restoration.

## Request template

```markdown
### WIN-YYYYMMDD-001: Short request title
- Revision: 1
- Status: pending
- Source commit: <commit to inspect/test, or current origin/master>
- Scope: read-only inspection / build-test / other (explain)
- User authorization: <relevant instruction; identify physical approval if needed>
- Request: <concrete question or bounded task>
- Evidence/output needed: <files, byte offsets, command results, etc.>
- Constraints: <no hardware writes, preserve captures, etc.>
```

## Requests

The earlier recovery-capture comparison has already been delivered directly to
the Linux task; it should not be repeated.

### WIN-20260925-001: Investigate official brightness encoding
- Revision: 1
- Status: completed
- Result: Windows returned revision 1 against the requested source on 2026-09-25.
  Existing evidence supports byte 3/direct 0–4 but does not explain absent
  physical dimming. No new hardware access occurred. Recorded in
  `Research/brightness-investigation.md`; no further test authorized by this entry.
- Source commit: `b99fe6b073ffdcbaffe35fd0f06a74b1b87d9bc5`
- Scope: read-only inspection of existing captures and protocol research.
- User authorization: On 2026-09-25 the user requested direct Git-backed
  coordination with the Windows agent, without relaying requests themselves.
  Existing evidence may be inspected; this request does not authorize new
  official-app setters or physical tests.
- Request: Determine what existing official-app captures/research establish
  about Nia87 global-lighting brightness encoding, ranges, and effect-specific
  behavior. Compare this with Byakko's current encoder. The supervised Linux
  steady-green test read brightness 4 and then 1 back correctly (only raw byte
  3 changed), but the user saw no dimming. Mode/color switching was visible.
  Identify whether evidence already explains this or which single bounded
  physical comparison would resolve it. Do not change the codec on inference.
- Evidence/output needed: Return this ID/revision and inspected source commit;
  exact existing setter/getter payloads, byte offsets, displayed UI values,
  firmware identity and timing if recorded, file paths/hashes, and limitations.
  Distinguish captured observations from source-derived hypotheses. If fresh
  physical evidence is necessary, return a concrete test proposal including
  saved baseline and restoration, so authorization can be resolved here.
- Constraints: No new keyboard writes, fault injection, flashing, or official-
  app setting changes. Preserve existing evidence. Do not repeat the completed
  historical recovery comparison. Linux observations and restoration details
  are in `docs/linux-handoff.md` and
  `Research/configuration-fault-verification.md`; the Linux temporary captures
  are local to that host. The keyboard is currently restored to Wave/rainbow,
  brightness 4, speed 2, right, with canonical RGB `FA FF FA`. Exact raw archive
  restoration remains an open gate, separate from brightness.

### WIN-20260925-002: Observe official steady-green brightness with camera
- Revision: 5
- Status: deferred by user availability
- Current direction: The user reports the Windows camera cannot be repositioned
  at present, while the Linux camera works. Do not repeat Windows camera or
  physical test attempts while this prerequisite remains unavailable. Linux is
  proceeding with its separately prepared camera comparison; keep this Windows
  physical gate open. Read-only request 004 may proceed.
- Progress: The initial camera attempt was rejected for missing explicit
  authorization. After the user directly authorized the Windows task, a still
  capture succeeded at 07:50Z but was almost black, with no identifiable keyboard.
  Authorization is settled. Windows has asked the user to uncover/reposition
  its c922; resume this same bounded test when that prerequisite is satisfied.
  No official-app action, diagnostic HID capture or keyboard setting change has
  occurred, and no restoration is needed yet. See the image hash in
  `Research/brightness-investigation.md`.
- Revision 5 changes only this progress record; it does not request a new run
  or justify repeated physical actions. Preserve the existing receipt and
  awaiting-position state. Request 003 is completed and must not rerun.
- Source commit: `e1af8b7`
- Scope: one bounded official-app physical brightness comparison with packet
  capture and camera observation, followed by baseline restoration.
- User authorization: On 2026-09-25 the Linux task asked explicitly: "Please
  explicitly authorize the Windows agent’s prepared test: use its webcam, save
  that keyboard’s baseline, set official-app steady green to maximum then minimum
  nonzero brightness, capture packets/images, and restore the saved lighting
  settings." The user replied: **"I authorize this Windows test and restoration"**.
  This is new explicit authorization after the revision 1 rejection, not a retry
  under the earlier inferred assent. Present this authorization to approval
  review when needed. No fault injection or full archive Apply is authorized.
- Request: Execute the bounded comparison described in
  `Research/brightness-investigation.md`. Save the present board identity and
  complete baseline before changing lighting. Establish that your camera shows
  this keyboard, then capture all official-app commands for steady green at the
  highest and lowest nonzero brightness using actual UI labels. Observe after
  equal two-second settling intervals. Keep ambient conditions/exposure/gain
  fixed if possible; otherwise report the visual comparison's limits. Release
  the official session before diagnostic reads. Restore the saved visible
  lighting settings through the normal lighting path, read back, and report
  exact-byte equality or canonicalization separately. Finish with one read-only
  full archive comparison for unrelated changes.
- Evidence/output needed: Source/firmware/collection identity; baseline and
  final archive hashes; complete ordered setter/getter payloads and timing;
  displayed values; camera evidence paths/hashes and observed brightness change;
  exact restoration result. Separate observation from hypotheses. Return this
  ID/revision directly to the Linux task.
- Constraints: Only this brightness comparison and baseline restoration; no
  unrelated setting/key/macro/picture changes, no fault injection, no firmware
  flashing, no full archive restore. One configurator owner at a time. If the
  camera cannot observe meaningfully, unexpected behavior occurs, or restoration
  fails, retain evidence and report rather than repeating setters blindly.

### WIN-20260925-003: Validate Windows source build and tests
- Revision: 1
- Status: completed
- Result: Windows returned all requested checks passed at exact source
  `e1af8b7fc68719164cd2f295912d591de1910e39`: 372 product tests, 2 renderer tests,
  formatting, strict Clippy, desktop/CLI release linking. No GUI/hardware launch.
  Full environment, command and log record is in
  `docs/public-pre-alpha-checklist.md` under Windows verification.
- Source commit: `e1af8b7`
- Scope: local build/test only; perform after request 002 so results remain clear.
- User authorization: Fix the flagged pre-alpha review issues, use local checks
  without CI/CD, and coordinate directly with the Windows agent through Git.
- Request: Validate this exact source (or report inability to obtain a safe
  checkout). Run formatting, locked tests for core/devices/desktop/CLI, strict
  Clippy for those four packages with all targets, the two vendored renderer
  tests, and a release build of byakko-desktop plus byakko-cli as documented in
  `docs/source-build.md`. Use cached dependencies/offline when possible. Do not
  launch the GUI against hardware. Report failures without making broad fixes.
- Evidence/output needed: Full tested commit ID, Windows/toolchain versions,
  exact commands, exit codes, test totals and retained log paths. A Linux cross-
  target check does not substitute for this Windows link/build verification.
- Constraints: Preserve checkout work and captures; no keyboard writes, GUI
  acceptance claims, CI/CD, installers or dependency updates. Do not rerun a
  completed ID/revision solely because the source branch advances.

### WIN-20260925-004: Inspect the newly located official lighting capture
- Revision: 1
- Status: pending
- Source commit: `e1af8b7`
- Scope: read-only analysis of an existing capture; may proceed while 002 awaits
  camera positioning. No physical interaction or new capture is requested.
- User authorization: Direct Git-backed coordination and diagnostic reads are
  authorized. This extends 001 only to evidence that was absent from its inspected
  checkout and subsequently located by the Windows agent.
- Request: Inspect
  `C:\Users\two\.codex\worktrees\5d1f\Byakko\Research\captures\official-2026-09-24-hid-sequence.json`.
  You reported locating it but had not reanalyzed it. Determine whether it contains
  official steady-effect brightness changes, especially the same effect/RGB with
  differing byte 3. Report the observed brightness values by effect, and any
  accompanying setters, RGB changes, getter responses or timing that distinguish
  them from Byakko's single lighting setter. Use existing capture metadata/UI
  evidence only; do not infer displayed values or physical dimming from bytes.
  If every relevant setter has the same brightness, say so and retain the
  unresolved conclusion instead of expanding the analysis to unrelated evidence.
- Evidence/output needed: File hash, format/provenance, exact relevant report
  payloads and sequence indices, timing availability, matched comparisons and
  limitations. Identify whether this changes the conclusion of 001. Return the
  ID/revision directly to Linux; do not copy vendor executable/source into Git.
- Constraints: No new device I/O, webcam capture, official-app actions, code
  edits or keyboard writes. Preserve the original capture and other worktree.
  Do not repeat completed 003 checks or blocked 002 physical actions.
