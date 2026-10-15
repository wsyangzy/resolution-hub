[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Executable,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{40}$')]
    [string]$Commit
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
$exePath = (Resolve-Path -LiteralPath $Executable).Path
& (Join-Path $PSScriptRoot 'Assert-StandaloneExecutable.ps1') -Executable $exePath
$bytes = [IO.File]::ReadAllBytes($exePath)
if ($bytes.Length -lt 256 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
    throw 'Executable is not a Windows PE file'
}
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
if ($pe -lt 0 -or $pe + 94 -gt $bytes.Length -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x4550) {
    throw 'Invalid PE header'
}
if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'Expected AMD64 executable' }
if ([BitConverter]::ToUInt16($bytes, $pe + 24) -ne 0x20B) { throw 'Expected PE32+ executable' }
if ([BitConverter]::ToUInt16($bytes, $pe + 24 + 68) -ne 2) { throw 'Expected Windows GUI subsystem' }

# Inspect resource types directly; shell icon extraction can return a generic icon for an EXE without resources.
$sectionCount = [BitConverter]::ToUInt16($bytes, $pe + 6)
$optionalSize = [BitConverter]::ToUInt16($bytes, $pe + 20)
$resourceRva = [BitConverter]::ToUInt32($bytes, $pe + 24 + 112 + 16)
$resourceOffset = $null
for ($section = 0; $section -lt $sectionCount; $section++) {
    $header = $pe + 24 + $optionalSize + 40 * $section
    $virtualSize = [BitConverter]::ToUInt32($bytes, $header + 8)
    $virtualAddress = [BitConverter]::ToUInt32($bytes, $header + 12)
    $rawSize = [BitConverter]::ToUInt32($bytes, $header + 16)
    if ($resourceRva -ge $virtualAddress -and $resourceRva -lt $virtualAddress + [Math]::Max($virtualSize, $rawSize)) {
        $resourceOffset = [BitConverter]::ToUInt32($bytes, $header + 20) + $resourceRva - $virtualAddress
        break
    }
}
if ($null -eq $resourceOffset) { throw 'PE resources are missing' }
$entryCount = [BitConverter]::ToUInt16($bytes, $resourceOffset + 12) + [BitConverter]::ToUInt16($bytes, $resourceOffset + 14)
$resourceTypes = @(for ($entry = 0; $entry -lt $entryCount; $entry++) {
    [BitConverter]::ToUInt32($bytes, $resourceOffset + 16 + 8 * $entry)
})
foreach ($required in @(3, 14, 16, 24)) {
    if ($required -notin $resourceTypes) { throw "Required PE resource type $required is missing" }
}

$manifestText = Get-Content -LiteralPath (Join-Path $repository 'Cargo.toml') -Raw
$versionMatch = [regex]::Match($manifestText, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"\s*$')
if (!$versionMatch.Success) { throw 'Workspace version is missing' }
$version = $versionMatch.Groups[1].Value
$info = [Diagnostics.FileVersionInfo]::GetVersionInfo($exePath)
if ($info.ProductVersion -ne $version -or $info.FileVersion -ne $version -or $info.ProductName -ne 'Resolution Hub') {
    throw "Unexpected executable version resources: $($info.ProductVersion) / $($info.FileVersion)"
}
$sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$mt = Get-ChildItem -LiteralPath $sdkBin -Directory | Sort-Object Name -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64/mt.exe' } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
if (!$mt) { throw 'Windows SDK mt.exe is missing' }
$temporary = Join-Path $env:RUNNER_TEMP ('resolution-hub-package-' + [guid]::NewGuid().ToString('N'))
$output = Join-Path $repository 'artifacts'
New-Item -ItemType Directory -Path $temporary -Force | Out-Null
New-Item -ItemType Directory -Path $output -Force | Out-Null
try {
    $extractedManifest = Join-Path $temporary 'app.manifest'
    & $mt '-nologo' ('-inputresource:' + $exePath + ';#1') ('-out:' + $extractedManifest)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot extract application manifest' }
    [xml]$xml = Get-Content -LiteralPath $extractedManifest -Raw
    $identities = @($xml.SelectNodes("//*[local-name()='assemblyIdentity']"))
    $controls = @($identities | Where-Object { $_.GetAttribute('name') -eq 'Microsoft.Windows.Common-Controls' -and $_.GetAttribute('version') -eq '6.0.0.0' })
    if ($controls.Count -ne 1) { throw 'Common-Controls v6 manifest dependency is missing or duplicated' }

    $publishedExe = Join-Path $output 'resolution-hub.exe'
    Copy-Item -LiteralPath $exePath -Destination $publishedExe
    $checksum = (Get-FileHash -LiteralPath $publishedExe -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output "Verified Windows x64 GUI executable, icon, version $version and Common-Controls v6 manifest."
    Write-Output "Executable: $publishedExe"
    Write-Output "SHA256: $checksum"
    if ($env:GITHUB_OUTPUT) {
        "version=$version", "sha256=$checksum" | Add-Content -LiteralPath $env:GITHUB_OUTPUT -Encoding utf8NoBOM
    }
    if ($env:GITHUB_STEP_SUMMARY) {
        "Verified Windows x64 GUI EXE, icon, version $version and Common-Controls v6 manifest. SHA: $Commit" |
            Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8NoBOM
    }
} finally {
    # Only remove this invocation's unique directory under the runner's temporary directory.
    $temporaryRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $temporaryPath = [IO.Path]::GetFullPath($temporary)
    if (!$temporaryPath.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to clean a directory outside RUNNER_TEMP'
    }
    Remove-Item -LiteralPath $temporaryPath -Recurse -Force
}
