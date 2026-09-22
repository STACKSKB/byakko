# WinDbg capture of final helper HID reports

Purpose: observe the **final** bytes passed by the supplied 32-bit
`iot_driver_v200` helper to `HidD_SetFeature`. The RPC proxy at port 3815 sees
an earlier boundary and cannot establish what the helper sends to Windows.
This is a capture plan, not a record of an attach or device test.

Execution follow-up: Microsoft WinDbg 1.2606.22001.0 was installed. Its package
includes x86 and amd64 CDB; Windows denied direct WindowsApps execution, so a
local ignored copy was used. The x86 debugger successfully captured the helper
with declared report length **67**, so the final bounded dump allowed lengths
1 through 256 rather than only 65. Breakpoints were cleared and the debugger
detached normally. See `protocol-web-fn.md` and `docs/live-evidence.md` for
the observations. Local-only symbol paths avoid long symbol-server stalls.

## Tool and process

Microsoft's debugger supports attaching to a live process by **decimal** PID
with `-p PID`; current WinDbg can be installed with
`winget install Microsoft.WinDbg`. On this host no CDB, WinDbg, USBPcap, or
ProcMon executable was found before the current installation attempt. After
installation, locate the package's actual debugger executable rather than
assuming `cdb.exe` is included. Use a full path to that executable. Do not
start or replace the helper: the official app starts its versioned helper
itself. Attach to the exact running PID, not the potentially ambiguous process
name.

From PowerShell, prepare a trace file by using .NET's atomic `CreateNew` mode;
it must be a fresh path. For example:

```powershell
$tracePath = Join-Path 'C:\Users\two\code\Byakko\Research\captures' ('hid-feature-' + [guid]::NewGuid().ToString('N') + '.log')
$traceFile = [System.IO.File]::Open($tracePath, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
$traceFile.Dispose()
$tracePath
```

The debugger should then use `.logappend <that exact path>`, **not**
`.logopen`, because `.logopen` overwrites an existing file. Keep a unique
script path for each run as well. A script can contain these debugger commands
once the x86 context and symbol have been verified:

```text
.expr /s masm
.logappend C:\Users\two\code\Byakko\Research\captures\REPLACE_WITH_NEW_FILE.log
bu hid!HidD_SetFeature ".if ((poi(@esp+0xc) > 0) & (poi(@esp+0xc) <= 0x41)) { .printf \"HidD_SetFeature len=%u thread=%x\n\", poi(@esp+0xc), @$tid; db poi(@esp+8) Lpoi(@esp+0xc) } .else { .printf \"HidD_SetFeature unusual length=%u (bytes skipped)\n\", poi(@esp+0xc) }; gc"
bl
g
```

`bu` is a deferred breakpoint, so the symbol can resolve if HID.DLL is loaded
later. At the x86 function entry, the `stdcall` arguments are the handle at
`ESP+4`, pointer at `ESP+8`, and length at `ESP+12`. `poi` reads a pointer-sized
value, which is four bytes in the verified x86 effective context. `db` dumps
exactly the declared byte count, capped at 65 for this bounded protocol trace;
longer or zero lengths are logged without reading the buffer. The report ID is
included. `gc` resumes after each logging hit. This command is **proposed**;
validate expression parsing and the first read-only hit before any Fn write.

Use WinDbg's attach dialog or the documented `-p` argument. The startup `-c`
option can run `$$><<path-to-script>` (PowerShell single quotes keep the `$`
characters literal), for example:

```powershell
& '<full path to WinDbg executable>' -pd -p <decimal helper PID> -c '$$><C:\Users\two\code\Byakko\Research\captures\hid-feature-commands.txt'
```

`-pd` is an extra safeguard that tells the debugger to leave the process
running when the session ends. For the **first** attach, omit `-c` and inspect the process manually before
executing the script. The debugger may initially show the WOW64 native x64
context. Run `r` and inspect the output. Switch to x86 with `.effmach x86`, or
load `wow64exts` and use `!wow64exts.sw` if the context still is not x86; then
confirm `r` includes `eip` and `esp`. Run `x hid!*SetFeature*` and confirm
`hid!HidD_SetFeature` resolves. Only then run the script with
`$$><C:\...\hid-feature-commands.txt` or repeat the attach with `-c` after
the debugger and context behavior are known. An x64 `RSP` breakpoint cannot
use these stack offsets.

Trigger one authorized, read-only helper action first. Check that the log
contains a plausible length and report ID. Then perform the specific Fn
operation under investigation and compare the captured final bytes with the
native application's corresponding 65-byte host report. The breakpoint
captures *every* call to this API in the helper, so isolate one UI operation
at a time. It does not prove which HID handle was used unless that handle is
correlated separately.

To stop, break into WinDbg, run `.logclose`, `bc *`, and `.detach` (or `qd`).
Both detach commands leave the helper running. Do not close the debugger while
the target is broken without first detaching. No trace file needs deletion.

References: Microsoft's [WinDbg installation](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/command-line-options),
[command-line options](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/windbg-command-line-options),
[WOW64 debugging](https://learn.microsoft.com/en-us/windows/win32/winprog64/debugging-wow64),
[script-file commands](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/-----------------------a---run-script-file-),
[breakpoint syntax](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/bp--bu--bm--set-breakpoint-),
[address-range expressions](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/address-and-address-range-syntax),
[log append](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/-logappend--append-log-file-),
[`gc`](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/gc--go-from-conditional-breakpoint-),
and [detach](https://learn.microsoft.com/en-us/windows-hardware/drivers/debuggercmds/-detach--detach-from-process-).
