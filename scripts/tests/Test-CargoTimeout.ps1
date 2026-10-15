$ErrorActionPreference = 'Stop'
$runner = Join-Path $PSScriptRoot '../Invoke-Cargo.ps1'
$standIn = (Get-Command python -CommandType Application | Select-Object -First 1).Source
# Substitute a short-lived child process; these cases never invoke Cargo or compile.
& {
    function Get-Command {
        param($Name, $CommandType)
        if ($Name -ne 'cargo') { throw 'Unexpected command lookup in Cargo runner' }
        [pscustomobject]@{ Source = $standIn }
    }
    & $runner -Arguments @('-c', 'pass') -TimeoutSeconds 10
    $failure = $null
    try { & $runner -Arguments @('-c', 'raise SystemExit(7)') -TimeoutSeconds 10 } catch { $failure = $_.Exception.Message }
    if ($failure -notlike '*exit code 7*') { throw "Cargo exit status was lost: $failure" }
    $failure = $null
    try { & $runner -Arguments @('-c', 'import time; time.sleep(30)') -TimeoutSeconds 1 } catch { $failure = $_.Exception.Message }
    if ($failure -notlike '*exceeded its 1 second timeout*') { throw "Cargo timeout did not fire: $failure" }
}
Write-Output 'Passed 3 Cargo process cases (success, failure and timeout), without compiling.'
