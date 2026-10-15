param(
    [Parameter(Mandatory = $true)]
    [string]$Path
)

$ErrorActionPreference = 'Stop'
$resolvedLog = (Resolve-Path -LiteralPath $Path).Path
$events = [System.Collections.Generic.List[object]]::new()
$malformedLines = 0
foreach ($line in [System.IO.File]::ReadLines($resolvedLog)) {
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    try { $events.Add(($line | ConvertFrom-Json -ErrorAction Stop)) }
    catch { $malformedLines++ }
}

function Measure-Values {
    param([string]$Name, [AllowEmptyCollection()][object[]]$Values)
    $samples = @($Values | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ } | Sort-Object)
    if ($samples.Count -eq 0) { return }
    $median = if ($samples.Count % 2 -eq 0) {
        ($samples[$samples.Count / 2 - 1] + $samples[$samples.Count / 2]) / 2
    } else { $samples[[int][Math]::Floor($samples.Count / 2)] }
    [pscustomobject]@{
        Name = $Name
        Samples = $samples.Count
        MedianMs = [Math]::Round($median, 3)
        P95Ms = [Math]::Round($samples[[int][Math]::Ceiling($samples.Count * 0.95) - 1], 3)
        MaxMs = [Math]::Round($samples[-1], 3)
    }
}

foreach ($group in ($events | Where-Object { $_.kind -eq 'work' } | Group-Object name)) {
    Measure-Values -Name $group.Name -Values @($group.Group.durationMs)
}
foreach ($group in ($events | Where-Object { $_.kind -eq 'render-cpu' } | Group-Object window)) {
    Measure-Values -Name ('render-cpu:' + $group.Name) -Values @($group.Group.durationMs)
}
foreach ($group in ($events | Where-Object { $_.kind -eq 'frame' } | Group-Object window)) {
    Measure-Values -Name ('input-to-render:' + $group.Name) -Values @($group.Group.inputToRenderMs)
    # 只统计连续两个采样点都处于浮层动画中的间隔，静止窗口不能按低 FPS 解读。
    $animated = @($group.Group | Where-Object { $_.animationInterval -eq $true })
    Measure-Values -Name ('animation-interval:' + $group.Name) -Values @($animated.intervalMs)
}
foreach ($group in ($events | Where-Object { $_.kind -eq 'monitor' } | Group-Object window)) {
    Measure-Values -Name ('monitor-frame-budget:' + $group.Name) -Values @($group.Group.expectedFrameMs)
}

if ($malformedLines -gt 0) {
    Write-Warning "Skipped $malformedLines malformed or incomplete log lines."
}
$dropped = ($events | Measure-Object -Property droppedRecords -Maximum).Maximum
if ($dropped -gt 0) {
    Write-Warning "Dropped $dropped diagnostic records; results are incomplete."
}
if (@($events | Where-Object { $_.kind -eq 'notifier-error' }).Count -gt 0) {
    Write-Warning 'Rendering notifications were unavailable; frame measurements are incomplete.'
}
