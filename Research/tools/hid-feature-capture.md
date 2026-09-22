# Capturing the helper's final HID feature-report bytes

The supplied `iot_driver_v200` helper is a 32-bit Windows process. A proxy at
its RPC boundary records the request *before* helper-side translation, so it
cannot settle whether the helper changed command, header, checksum, or payload
before calling Windows HID. The observation point is the entry to
`HidD_SetFeature(handle, report_buffer, report_buffer_length)`. On x86, the
three arguments are at `ESP+4`, `ESP+8`, and `ESP+12` when that function begins.
The report starts at the pointer in `ESP+8`; its actual length is the value in
`ESP+12`. Capture the length and exactly that many bytes, including the report
ID. Correlate with a single controlled UI operation and distinguish calls for
other HID devices if there are any.

## Recommended route

Use Microsoft's WinDbg on the exact running helper PID. `cdb`, WinDbg,
USBPcap, ProcMon, and ProcDump were not present on this host when checked on
2026-09-22; `winget` is available. Microsoft documents
`winget install Microsoft.WinDbg` for its current debugger. Installation and
the subsequent live attach are separate actions; neither has been performed
here. WinDbg owns software-breakpoint reinsertion, debug events, and detach
cleanup, making it substantially safer than an original one-off debugger.

Proposed WinDbg workflow (commands require validation against the actual
attached 32-bit context before triggering a keyboard operation):

1. Start the official app and identify the PID of its live versioned
   `iot_driver_*.exe`. Attach WinDbg to that PID. Verify `HID.DLL` is loaded
   and that the effective register context is x86 (`r`; `x hid!*SetFeature*`).
   If WinDbg shows the WOW64 host context, switch to x86 using its documented
   WOW64 effective-machine/context command before interpreting `ESP`.
2. Open a **new, unique** log path under `Research/captures/` with
   `.logopen /t <path>`. `/t` adds PID and timestamp but is not a strict
   create-new guarantee: check the resulting name before use. The log may
   contain raw keyboard reports, so keep it local.
3. Set a logging breakpoint on `hid!HidD_SetFeature` whose command prints
   `poi(@esp+0xc)` as the length, then dumps bytes from `poi(@esp+8)` for
   that exact length, and finally resumes with `gc`. The candidate WinDbg
   command is:

   ```text
   bp hid!HidD_SetFeature ".printf \"HidD_SetFeature length=%u\\n\", poi(@esp+0xc); db poi(@esp+8) Lpoi(@esp+0xc); gc"
   ```

   Confirm that the expression evaluator accepts the dynamic `L` count in
   the installed debugger before triggering a write. If it does not, break
   without auto-resume and inspect `dd @esp L4`, then issue a manual `db`
   with the observed count. Never substitute a larger fixed count, because
   that can read unrelated process memory past the buffer.
4. Trigger one known read-only app operation first to confirm a plausible
   report length and report ID. For the Fn investigation, perform only the
   independently authorized test, then compare the captured bytes at this
   final Windows API boundary with the native application's bytes. Close the
   log, clear the breakpoint, and detach using the debugger's detach command.

Microsoft documents [WinDbg installation](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/command-line-options),
[breakpoint command strings](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/bp--bu--bm--set-breakpoint-),
[memory display/range syntax](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/address-and-address-range-syntax),
[logging](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/-logopen--open-log-file-),
and [`gc` continuation](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/gc--go-from-conditional-breakpoint-).

## Why a custom binary was deferred

A 64-bit Rust tool could use `DebugActiveProcess`, `TH32CS_SNAPMODULE32`,
`Wow64GetThreadContext`, remote memory reads, and an x86 export lookup to
place an `INT3` at the 32-bit `HidD_SetFeature` entry. It then has to restore
the original byte, rewind EIP, single-step, reinsert `INT3`, account for other
threads, continue every debug event correctly, and detach even after an error
or interruption. A failure in that state machine can leave the official helper
paused or patched. The current Rust toolchain has only x64 Windows and x64
Linux targets installed. There is no existing debugger engine dependency in
this project. Implementing all of that solely for a short diagnostic trace
would be more fragile than using Microsoft's maintained debugger.

The cross-bitness primitives are documented by Microsoft:
[`TH32CS_SNAPMODULE32`](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot)
and [`Wow64GetThreadContext`](https://learn.microsoft.com/en-us/windows/win32/api/wow64apiset/nf-wow64apiset-wow64getthreadcontext).
No debugger, helper, or device was launched or attached while preparing this
note.
