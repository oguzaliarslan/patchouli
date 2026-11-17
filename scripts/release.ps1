[CmdletBinding()]
param(
    [switch]$SignedUpdates,
    [switch]$NsisOnly,
    [string]$Repository = 'oguzaliarslan/patchouli',
    [string]$Notes = ''
)

$ErrorActionPreference = 'Stop'
if ($Repository -notmatch '^[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+$') {
    throw 'Repository must be owner/repo.'
}
if ($SignedUpdates -and -not ($env:TAURI_SIGNING_PRIVATE_KEY -and $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
    throw 'Set TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD before building signed updates.'
}
$certificate = $env:PATCHOULI_SIGNING_THUMBPRINT
if ($certificate -and $certificate -notmatch '^[A-Fa-f0-9]{40}$') {
    throw 'PATCHOULI_SIGNING_THUMBPRINT must be a 40-digit hexadecimal certificate thumbprint.'
}

$previousRepo = $env:TRACKER_GITHUB_REPO
$previousTarget = $env:CARGO_TARGET_DIR
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $env:TRACKER_GITHUB_REPO = $Repository
    $env:CARGO_TARGET_DIR = Join-Path (Get-Location).Path 'target'
    $config = Get-Content -Raw src-tauri/tauri.conf.json | ConvertFrom-Json
    $version = $config.version
    $output = New-Item -ItemType Directory -Force "target/releases/v$version-$(Get-Date -Format yyyyMMdd-HHmmssfff)"
    $bundleTypes = if ($NsisOnly) { @('nsis') } else { @('nsis', 'msi') }
    $override = @{ bundle = @{ createUpdaterArtifacts = [bool]$SignedUpdates } }
    if ($certificate) {
        $override.bundle.windows = @{
            certificateThumbprint = $certificate
            digestAlgorithm = 'sha256'
            timestampUrl = 'http://timestamp.digicert.com'
            tsp = $true
        }
    }
    $configPath = Join-Path $output.FullName 'build-config.json'
    [System.IO.File]::WriteAllText($configPath, ($override | ConvertTo-Json -Depth 6))
    $started = [DateTime]::UtcNow
    & npm.cmd run tauri -- build --target x86_64-pc-windows-msvc --bundles ($bundleTypes -join ',') --config $configPath
    if ($LASTEXITCODE -ne 0) { throw "Tauri build failed ($LASTEXITCODE)." }

    $release = Join-Path $env:CARGO_TARGET_DIR 'x86_64-pc-windows-msvc/release'
    $platforms = @{}
    foreach ($type in $bundleTypes) {
        $pattern = if ($type -eq 'nsis') { "Patchouli_${version}_*-setup.exe" } else { "Patchouli_${version}_*.msi" }
        $asset = Get-ChildItem -LiteralPath (Join-Path $release "bundle/$type") -Filter $pattern -File |
            Where-Object { $_.LastWriteTimeUtc -ge $started } | Select-Object -First 1
        if (-not $asset) { throw "The build did not produce a fresh $type installer." }
        Copy-Item -LiteralPath $asset.FullName -Destination $output.FullName
        if ($SignedUpdates) {
            $signature = Get-Item -LiteralPath "$($asset.FullName).sig"
            if ($signature.LastWriteTimeUtc -lt $started) { throw "The $type signature is stale." }
            Copy-Item -LiteralPath $signature.FullName -Destination $output.FullName
            $platforms["windows-x86_64-$type"] = @{
                url = "https://github.com/$Repository/releases/download/v$version/$($asset.Name)"
                signature = (Get-Content -Raw -LiteralPath $signature.FullName).Trim()
            }
        }
    }

    $portable = New-Item -ItemType Directory (Join-Path $output.FullName 'portable')
    Copy-Item -LiteralPath (Join-Path $release 'patchouli.exe') -Destination $portable.FullName
    Copy-Item -LiteralPath 'src-tauri/resources' -Destination $portable.FullName -Recurse
    Copy-Item -LiteralPath 'THIRD_PARTY.md' -Destination $portable.FullName
    if (Test-Path -LiteralPath 'LICENSE') {
        Copy-Item -LiteralPath 'LICENSE' -Destination $portable.FullName
    }
    New-Item -ItemType File (Join-Path $portable.FullName 'portable') | Out-Null
    Compress-Archive -Path (Join-Path $portable.FullName '*') -DestinationPath (Join-Path $output.FullName "Patchouli_${version}_x64-portable.zip")

    if ($SignedUpdates) {
        $platforms['windows-x86_64'] = $platforms['windows-x86_64-nsis']
        $manifest = @{
            version = $version
            notes = $Notes
            pub_date = [DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'")
            platforms = $platforms
        } | ConvertTo-Json -Depth 6
        [System.IO.File]::WriteAllText((Join-Path $output.FullName 'latest.json'), $manifest)
    }
    Write-Host "Release files: $($output.FullName)"
}
finally {
    $env:TRACKER_GITHUB_REPO = $previousRepo
    $env:CARGO_TARGET_DIR = $previousTarget
    Pop-Location
}
