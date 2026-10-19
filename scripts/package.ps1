#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')]
    [string]$Version,
    [string]$BinaryPath,
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if (-not $BinaryPath) { $BinaryPath = Join-Path $repoRoot 'target/release/studyapp.exe' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repoRoot 'dist' }
$binary = (Resolve-Path -LiteralPath $BinaryPath).Path
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'BinaryPath must name one existing file' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
$name = "StudyApp-$Version-windows-x86_64"
$staging = Join-Path $output $name
$archive = Join-Path $output "$name.zip"
$checksum = "$archive.sha256"
foreach ($path in @($staging, $archive, $checksum)) {
    if (Test-Path -LiteralPath $path) { throw "Output already exists; choose a fresh OutputDirectory: $path" }
}

Push-Location -LiteralPath $repoRoot
try {
    # Filter to the supported binary target, including all features for a conservative notice set.
    $metadata = cargo metadata --locked --offline --all-features --filter-platform x86_64-pc-windows-msvc --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Locked Cargo metadata failed; build the Windows application first' }
    $workspace = @($metadata.packages | Where-Object { $_.id -in $metadata.workspace_members })
    if (@($workspace | Where-Object { $_.version -cne $Version }).Count -gt 0) { throw 'Version differs from workspace package versions' }
    [IO.Directory]::CreateDirectory($staging) | Out-Null
    Copy-Item -LiteralPath $binary -Destination (Join-Path $staging 'studyapp.exe')
    foreach ($file in @('README.md', 'LICENSE')) {
        Copy-Item -LiteralPath (Join-Path $repoRoot $file) -Destination (Join-Path $staging $file)
    }
    Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/release-notes.md') -Destination (Join-Path $staging 'RELEASE-NOTES.md')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'Cargo.lock') -Destination (Join-Path $staging 'Cargo.lock')
    [IO.Directory]::CreateDirectory((Join-Path $staging 'docs')) | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/cloud-contract.md') -Destination (Join-Path $staging 'docs/cloud-contract.md')
    if (Test-Path -LiteralPath (Join-Path $repoRoot 'docs/verification.md') -PathType Leaf) {
        Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/verification.md') -Destination (Join-Path $staging 'docs/verification.md')
    }
    if (Test-Path -LiteralPath (Join-Path $repoRoot 'docs/images/dashboard.png') -PathType Leaf) {
        [IO.Directory]::CreateDirectory((Join-Path $staging 'docs/images')) | Out-Null
        Copy-Item -LiteralPath (Join-Path $repoRoot 'docs/images/dashboard.png') -Destination (Join-Path $staging 'docs/images/dashboard.png')
    }
    # Several upstream crates omit standard license text from their published archives.
    # Preserve a shared Apache-2.0 text as well as all crate-specific notices supplied.
    $anyhow = $metadata.packages | Where-Object { $_.name -eq 'anyhow' } | Select-Object -First 1
    $apacheLicense = Join-Path (Split-Path -Parent $anyhow.manifest_path) 'LICENSE-APACHE'
    Copy-Item -LiteralPath $apacheLicense -Destination (Join-Path $staging 'LICENSE-APACHE-2.0.txt')
    $notices = [Collections.Generic.List[string]]::new()
    $notices.Add("# Third-party notices`n`nDependencies below are conservatively collected from the locked Windows graph, including development/test features. Each dependency retains its own license. StudyApp does not modify their sources. Exact source archives are linked for source availability, including MPL-2.0 components. Shared Apache-2.0 license text is in LICENSE-APACHE-2.0.txt; crate-specific notices are copied where supplied in source archives.`n")
    $dependencies = @($metadata.packages | Where-Object { $_.source } | Sort-Object name,version)
    foreach ($package in $dependencies) {
        if ($package.source -notmatch '^registry\+') { throw "Review non-registry dependency notices manually: $($package.id)" }
        $notices.Add("## $($package.name) $($package.version)`n`nLicense expression: $($package.license)`n`nSource: https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download`n")
        if ($package.repository) { $notices.Add("Upstream: $($package.repository)`n") }
        if ($package.authors.Count) { $notices.Add("Declared authors: $($package.authors -join ', ')`n") }
        $root = Split-Path -Parent $package.manifest_path
        $licenseFiles = @(Get-ChildItem -LiteralPath $root -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|AUTHORS)($|[._-])' })
        if ($package.license_file) {
            $declaredFile = Get-Item -LiteralPath (Join-Path $root $package.license_file)
            if ($declaredFile.FullName -notin @($licenseFiles | ForEach-Object FullName)) { $licenseFiles += $declaredFile }
        }
        if ($licenseFiles.Count -eq 0) {
            # Some crates distribute notices in a licenses/ directory.
            $licenseDirectory = Join-Path $root 'licenses'
            if (Test-Path -LiteralPath $licenseDirectory -PathType Container) {
                $licenseFiles = @(Get-ChildItem -LiteralPath $licenseDirectory -File -Recurse)
            }
        }
        foreach ($file in ($licenseFiles | Sort-Object FullName -Unique)) {
            $relative = [IO.Path]::GetRelativePath($root, $file.FullName)
            $destination = Join-Path $staging "third-party/$($package.name)-$($package.version)/$relative"
            [IO.Directory]::CreateDirectory((Split-Path -Parent $destination)) | Out-Null
            Copy-Item -LiteralPath $file.FullName -Destination $destination
            $notices.Add("Notice: third-party/$($package.name)-$($package.version)/$($relative.Replace('\', '/'))`n")
        }
    }
    [IO.File]::WriteAllText((Join-Path $staging 'THIRD-PARTY-NOTICES.md'), ($notices -join "`n"), [Text.UTF8Encoding]::new($false))
    # ZipFile accepts literal directory names; it does not interpret wildcard paths.
    [IO.Compression.ZipFile]::CreateFromDirectory($staging, $archive, [IO.Compression.CompressionLevel]::Optimal, $true)
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($checksum, "$hash  $name.zip`n", [Text.UTF8Encoding]::new($false))
    Write-Output "ZIP: $archive"
    Write-Output "SHA256: $checksum"
} finally {
    Pop-Location
}
