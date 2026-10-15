[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Executable
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$bytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Executable).Path)

function Assert-Range([long]$Offset, [long]$Length) {
    if ($Offset -lt 0 -or $Length -lt 0 -or $Offset + $Length -gt $bytes.Length) {
        throw 'PE data extends beyond the file'
    }
}

Assert-Range 0 64
if ($bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) { throw 'Expected a Windows PE file' }
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
Assert-Range $pe 24
if ([BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550) { throw 'Invalid PE signature' }
if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'Expected AMD64 executable' }
$sectionCount = [BitConverter]::ToUInt16($bytes, $pe + 6)
$optionalSize = [BitConverter]::ToUInt16($bytes, $pe + 20)
$optional = $pe + 24
Assert-Range $optional $optionalSize
if ($optionalSize -lt 112 -or [BitConverter]::ToUInt16($bytes, $optional) -ne 0x20B) {
    throw 'Expected PE32+ optional header'
}
$directoryCount = [BitConverter]::ToUInt32($bytes, $optional + 108)
if ($directoryCount -lt 2 -or 112 + 8 * $directoryCount -gt $optionalSize) {
    throw 'Invalid PE data directory count'
}
$sections = @(for ($i = 0; $i -lt $sectionCount; $i++) {
    $header = $optional + $optionalSize + 40 * $i
    Assert-Range $header 40
    [pscustomobject]@{
        Rva = [BitConverter]::ToUInt32($bytes, $header + 12)
        Size = [BitConverter]::ToUInt32($bytes, $header + 16)
        Offset = [BitConverter]::ToUInt32($bytes, $header + 20)
    }
})

function ConvertTo-FileOffset([long]$Rva, [int]$Length) {
    foreach ($section in $sections) {
        $relative = $Rva - $section.Rva
        if ($relative -ge 0 -and $relative + $Length -le $section.Size) {
            $offset = [long]$section.Offset + $relative
            Assert-Range $offset $Length
            return $offset
        }
    }
    throw "PE address cannot be mapped to file data: $Rva"
}

$modules = [Collections.Generic.List[string]]::new()
# Check both eager and delay-loaded imports: the latter can fail only on first use.
foreach ($table in @(@{ Index = 1; Stride = 20; NameAt = 12 }, @{ Index = 13; Stride = 32; NameAt = 4 })) {
    if ($table.Index -ge $directoryCount) { continue }
    $directory = $optional + 112 + 8 * $table.Index
    $rva = [BitConverter]::ToUInt32($bytes, $directory)
    $size = [BitConverter]::ToUInt32($bytes, $directory + 4)
    if ($rva -eq 0 -and $size -eq 0) { continue }
    if ($rva -eq 0 -or $size -lt $table.Stride) { throw 'Invalid PE import directory' }
    $terminated = $false
    for ($entry = 0; $entry + $table.Stride -le $size; $entry += $table.Stride) {
        $cursor = ConvertTo-FileOffset ($rva + $entry) $table.Stride
        $empty = $true
        for ($b = 0; $b -lt $table.Stride; $b++) {
            if ($bytes[$cursor + $b] -ne 0) { $empty = $false; break }
        }
        if ($empty) { $terminated = $true; break }
        if ($table.Index -eq 13 -and [BitConverter]::ToUInt32($bytes, $cursor) -ne 1) {
            throw 'Expected RVA-based PE32+ delay imports'
        }
        $nameRva = [BitConverter]::ToUInt32($bytes, $cursor + $table.NameAt)
        $name = [Collections.Generic.List[byte]]::new()
        # A module name cannot legitimately need more than the Win32 path limit.
        for ($n = 0; $n -lt 260; $n++) {
            $character = $bytes[(ConvertTo-FileOffset ($nameRva + $n) 1)]
            if ($character -eq 0) { break }
            $name.Add($character)
        }
        if ($name.Count -eq 0 -or $n -eq 260) { throw 'Invalid PE import module name' }
        $modules.Add([Text.Encoding]::ASCII.GetString($name.ToArray()))
    }
    if (!$terminated) { throw 'Unterminated PE import directory' }
}
if ($modules.Count -eq 0) { throw 'No PE imports found; refusing an empty dependency check' }
$forbidden = @($modules | Where-Object {
    $_ -match '^(vcruntime|msvcp|msvcr|concrt|vcamp|vcomp)\d[^/\\]*\.dll$'
})
if ($forbidden.Count -gt 0) {
    throw "EXE requires the Visual C++ redistributable: $($forbidden -join ', '). Check +crt-static and RUSTFLAGS."
}
Write-Output "Verified $($modules.Count) PE imports (including delay imports): no Visual C++ redistributable dependency."
