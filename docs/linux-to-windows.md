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

No pending requests at setup. The earlier recovery-capture comparison has already
been delivered directly to the Linux task; it should not be repeated.
