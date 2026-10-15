$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$checker = Join-Path $PSScriptRoot '../Assert-StandaloneExecutable.ps1'
$temporaryRoot = [IO.Path]::GetTempPath()
$temporary = Join-Path $temporaryRoot ('resolution-hub-import-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null

function New-Fixture([string]$Module, [switch]$Delay, [switch]$Truncated, [switch]$Unmapped, [switch]$Empty) {
    $bytes = [byte[]]::new(2048)
    $bytes[0] = 0x4D
    $bytes[1] = 0x5A
    [BitConverter]::GetBytes([int]0x80).CopyTo($bytes, 0x3C)
    [BitConverter]::GetBytes([int]0x4550).CopyTo($bytes, 0x80)
    [BitConverter]::GetBytes([uint16]0x8664).CopyTo($bytes, 0x84)
    [BitConverter]::GetBytes([uint16]1).CopyTo($bytes, 0x86)
    [BitConverter]::GetBytes([uint16]240).CopyTo($bytes, 0x94)
    $optional = 0x98
    [BitConverter]::GetBytes([uint16]0x20B).CopyTo($bytes, $optional)
    [BitConverter]::GetBytes([int]16).CopyTo($bytes, $optional + 108)
    $section = $optional + 240
    [BitConverter]::GetBytes([int]0x1000).CopyTo($bytes, $section + 12)
    [BitConverter]::GetBytes([int]0x600).CopyTo($bytes, $section + 16)
    [BitConverter]::GetBytes([int]0x200).CopyTo($bytes, $section + 20)
    if (!$Empty) {
        $index = if ($Delay) { 13 } else { 1 }
        $stride = if ($Delay) { 32 } else { 20 }
        $nameAt = if ($Delay) { 4 } else { 12 }
        [BitConverter]::GetBytes([int]0x1000).CopyTo($bytes, $optional + 112 + 8 * $index)
        $size = if ($Truncated) { $stride } else { 2 * $stride }
        [BitConverter]::GetBytes([int]$size).CopyTo($bytes, $optional + 116 + 8 * $index)
        if ($Delay) { $bytes[0x200] = 1 }
        $nameRva = if ($Unmapped) { 0x9000 } else { 0x1100 }
        [BitConverter]::GetBytes([int]$nameRva).CopyTo($bytes, 0x200 + $nameAt)
        [Text.Encoding]::ASCII.GetBytes($Module).CopyTo($bytes, 0x300)
    }
    $fixture = Join-Path $temporary 'fixture.exe'
    [IO.File]::WriteAllBytes($fixture, $bytes)
    return $fixture
}

function Assert-Rejected([string]$Path, [string]$Expected) {
    $failure = $null
    try { & $checker -Executable $Path | Out-Null } catch { $failure = $_.Exception.Message }
    if (!$failure -or $failure -notlike "*$Expected*") { throw "Expected rejection containing '$Expected'; got '$failure'" }
}

try {
    & $checker -Executable (New-Fixture 'KERNEL32.dll')
    & $checker -Executable (New-Fixture 'USER32.dll' -Delay)
    # The UCRT is part of Windows 10/11, unlike the separate VC++ redistributable.
    & $checker -Executable (New-Fixture 'api-ms-win-crt-runtime-l1-1-0.dll')
    Assert-Rejected (New-Fixture 'VCRUNTIME140.dll') 'Visual C++ redistributable'
    Assert-Rejected (New-Fixture 'MSVCP140_1.dll' -Delay) 'Visual C++ redistributable'
    Assert-Rejected (New-Fixture 'KERNEL32.dll' -Truncated) 'Unterminated'
    Assert-Rejected (New-Fixture 'KERNEL32.dll' -Unmapped) 'cannot be mapped'
    Assert-Rejected (New-Fixture '' -Empty) 'No PE imports'
    [IO.File]::WriteAllBytes((Join-Path $temporary 'invalid.exe'), [byte[]]::new(4))
    Assert-Rejected (Join-Path $temporary 'invalid.exe') 'beyond the file'
    Write-Output 'Passed 9 PE import-check cases (normal/delay imports, VC++ rejection and malformed files).'
} finally {
    $resolved = [IO.Path]::GetFullPath($temporary)
    $root = [IO.Path]::GetFullPath($temporaryRoot).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (!$resolved.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
