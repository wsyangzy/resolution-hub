param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [string]$LogDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) ('resolution-hub-perf-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))),
    [switch]$DisableAnimations
)

$ErrorActionPreference = 'Stop'
$exePath = (Resolve-Path -LiteralPath $Executable).Path
if (!(Test-Path -LiteralPath $exePath -PathType Leaf)) { throw "Executable not found: $exePath" }
if (Get-Process -Name resolution-hub-native-ui,resolution-hub -ErrorAction SilentlyContinue) {
    throw 'End the game session and exit the existing Resolution Hub application before measuring.'
}
$sessionDirectory = Join-Path $LogDirectory ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $sessionDirectory | Out-Null
$frameLog = Join-Path $sessionDirectory 'performance.jsonl'
$processLog = Join-Path $sessionDirectory 'process.csv'
$previousLog = [Environment]::GetEnvironmentVariable('RESOLUTION_HUB_PERF_LOG', 'Process')
$previousAnimations = [Environment]::GetEnvironmentVariable('RESOLUTION_HUB_DISABLE_ANIMATIONS', 'Process')
$writer = [System.IO.StreamWriter]::new($processLog, $false, [System.Text.UTF8Encoding]::new($false))
$measuredProcess = $null

try {
    $env:RESOLUTION_HUB_PERF_LOG = $frameLog
    if ($DisableAnimations) { $env:RESOLUTION_HUB_DISABLE_ANIMATIONS = '1' }
    else { [Environment]::SetEnvironmentVariable('RESOLUTION_HUB_DISABLE_ANIMATIONS', $null, 'Process') }
    $measuredProcess = Start-Process -FilePath $exePath -WindowStyle Normal -PassThru
    $writer.WriteLine('ElapsedSeconds,CpuPercent,WorkingSetMiB,PrivateMiB,Handles,Threads')
    $elapsed = [System.Diagnostics.Stopwatch]::StartNew()
    $previousTime = $elapsed.Elapsed.TotalSeconds
    $previousCpu = $measuredProcess.TotalProcessorTime.TotalSeconds
    Write-Host "PID: $($measuredProcess.Id)"
    Write-Host "Logs: $sessionDirectory"
    Write-Host 'Repeat the same interactions, then leave the app idle for 30 seconds and close it normally.'
    while (!$measuredProcess.WaitForExit(1000)) {
        $measuredProcess.Refresh()
        if ($measuredProcess.HasExited) { break }
        $now = $elapsed.Elapsed.TotalSeconds
        $cpu = $measuredProcess.TotalProcessorTime.TotalSeconds
        $cpuPercent = 100.0 * ($cpu - $previousCpu) / [Math]::Max(0.001, $now - $previousTime) / [Environment]::ProcessorCount
        $sample = @($now, $cpuPercent, ($measuredProcess.WorkingSet64 / 1MB), ($measuredProcess.PrivateMemorySize64 / 1MB), $measuredProcess.HandleCount, $measuredProcess.Threads.Count)
        $writer.WriteLine(($sample | ForEach-Object { ([double]$_).ToString('0.000', [Globalization.CultureInfo]::InvariantCulture) }) -join ',')
        $writer.Flush()
        $previousTime = $now
        $previousCpu = $cpu
    }
} finally {
    $writer.Dispose()
    if ($null -ne $measuredProcess) { $measuredProcess.Dispose() }
    [Environment]::SetEnvironmentVariable('RESOLUTION_HUB_PERF_LOG', $previousLog, 'Process')
    [Environment]::SetEnvironmentVariable('RESOLUTION_HUB_DISABLE_ANIMATIONS', $previousAnimations, 'Process')
}

& (Join-Path $PSScriptRoot 'Summarize-Performance.ps1') -Path $frameLog | Format-Table -AutoSize
Write-Host "Process samples: $processLog"
