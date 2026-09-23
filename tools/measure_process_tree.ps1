param(
    [Parameter(Mandatory = $true)]
    [int] $RootPid,

    [ValidateRange(1, 300)]
    [int] $Seconds = 15
)

$ErrorActionPreference = 'Stop'

function Get-TreeSample([int] $ProcessId) {
    $processes = @(Get-CimInstance Win32_Process)
    $selected = @($ProcessId)
    do {
        $count = $selected.Count
        foreach ($process in $processes) {
            if ($selected -contains [int] $process.ParentProcessId -and
                $selected -notcontains [int] $process.ProcessId) {
                $selected += [int] $process.ProcessId
            }
        }
    } while ($selected.Count -ne $count)

    $members = @($processes | Where-Object { $selected -contains [int] $_.ProcessId })
    if (-not ($members | Where-Object { [int] $_.ProcessId -eq $ProcessId })) {
        throw "Process $ProcessId is no longer running"
    }
    return $members
}

$before = @(Get-TreeSample $RootPid)
Start-Sleep -Seconds $Seconds
$after = @(Get-TreeSample $RootPid)
$stable = @($after | Where-Object { $before.ProcessId -contains $_.ProcessId })
$started = @($after | Where-Object { $before.ProcessId -notcontains $_.ProcessId })
$exited = @($before | Where-Object { $after.ProcessId -notcontains $_.ProcessId })

$cpuTicks = [long] 0
foreach ($process in $stable) {
    $previous = $before | Where-Object { $_.ProcessId -eq $process.ProcessId } | Select-Object -First 1
    $cpuTicks += ([long] $process.UserModeTime + [long] $process.KernelModeTime) -
        ([long] $previous.UserModeTime + [long] $previous.KernelModeTime)
}

[pscustomobject]@{
    root_pid = $RootPid
    interval_seconds = $Seconds
    process_count = $after.Count
    process_names = @($after | ForEach-Object { $_.Name })
    working_set_mib = [math]::Round((($after | Measure-Object WorkingSetSize -Sum).Sum / 1MB), 1)
    private_pages_mib = [math]::Round((($after | Measure-Object PrivatePageCount -Sum).Sum / 1MB), 1)
    cpu_seconds_per_minute = [math]::Round(($cpuTicks / 10000000.0) * (60.0 / $Seconds), 3)
    started_pids = @($started | ForEach-Object { $_.ProcessId })
    exited_pids = @($exited | ForEach-Object { $_.ProcessId })
} | ConvertTo-Json -Depth 3
