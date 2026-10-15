[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Arguments,
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 3600)]
    [int]$TimeoutSeconds
)

$ErrorActionPreference = 'Stop'
# Composite actions do not expose step timeout-minutes. Bound each Cargo process
# separately so a hung test fails with its log before the overall job is killed.
$cargo = Get-Command cargo -CommandType Application | Select-Object -First 1
$start = [Diagnostics.ProcessStartInfo]::new($cargo.Source)
$start.UseShellExecute = $false
foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
try {
    if (!$process.Start()) { throw 'Cannot start Cargo' }
    if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
        $process.Kill($true)
        $process.WaitForExit()
        throw "Cargo exceeded its $TimeoutSeconds second timeout: $($Arguments -join ' ')"
    }
    if ($process.ExitCode -ne 0) { throw "Cargo failed with exit code $($process.ExitCode)" }
} finally {
    $process.Dispose()
}
