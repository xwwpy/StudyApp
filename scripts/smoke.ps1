#requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$DataDirectory,
    [string]$BinaryPath = (Join-Path $PSScriptRoot '../target/debug/studyapp.exe')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $BinaryPath).Path
$data = [IO.Path]::GetFullPath($DataDirectory)
if (Test-Path -LiteralPath $data) { throw 'Smoke directory must be fresh; no existing fixtures are changed' }
$stdout = "$data.stdout.log"
$stderr = "$data.stderr.log"
$process = Start-Process -FilePath $binary -ArgumentList @('--smoke-test', '--data-dir', ('"' + $data + '"')) -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
if (-not $process.WaitForExit(150000)) { $process.Kill(); throw 'Native smoke exceeded 150 seconds' }
if (Test-Path -LiteralPath $data -PathType Container) {
    Copy-Item -LiteralPath $stdout -Destination (Join-Path $data 'smoke.stdout.log')
    Copy-Item -LiteralPath $stderr -Destination (Join-Path $data 'smoke.stderr.log')
}
if ($process.ExitCode -ne 0) {
    Get-Content -LiteralPath $stderr -Tail 50
    throw "Native smoke exit code: $($process.ExitCode)"
}
$report = Join-Path $data 'smoke-result.txt'
if (-not (Test-Path -LiteralPath $report)) { throw 'Smoke report missing' }
if ((Get-Content -LiteralPath $report -Raw) -notmatch '^PASS:') { throw 'Smoke report did not pass' }
$libraryReport = Join-Path $data 'library-smoke-result.txt'
if ((Get-Content -LiteralPath $libraryReport -Raw) -notmatch '^PASS:') { throw 'Library and history smoke report did not pass' }
$interactionsReport = Join-Path $data 'interactions-smoke-result.txt'
if ((Get-Content -LiteralPath $interactionsReport -Raw) -notmatch '^PASS:') { throw 'Note interactions and scroll smoke report did not pass' }
foreach ($nativeImage in @('native-first-reader.bmp', 'native-first-editor.bmp', 'native-after-history.bmp', 'native-after-menu.bmp', 'native-after-settings.bmp', 'native-after-resize.bmp')) {
    $nativePath = Join-Path $data $nativeImage
    if (-not (Test-Path -LiteralPath $nativePath) -or (Get-Item -LiteralPath $nativePath).Length -lt 1000) {
        throw "Native screen capture missing or empty: $nativeImage"
    }
}
$draft = Get-Content -LiteralPath (Join-Path $data 'note-draft.json') -Raw | ConvertFrom-Json
if ($draft.content -cne '关闭窗口前的末次输入 🧠') { throw 'Normal window close did not persist the last editor input' }
foreach ($index in 0..4) {
    $image = Join-Path $data "page-$index.png"
    if (-not (Test-Path -LiteralPath $image)) { throw "Screenshot missing: $image" }
    if ((Get-Item -LiteralPath $image).Length -lt 1000) { throw "Screenshot is empty: $image" }
}
Write-Output "Native smoke passed; evidence retained at $data"
