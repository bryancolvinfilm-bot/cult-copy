$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskDirectory = Join-Path $taskRoot ('.tools\release-check-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskDirectory -Force | Out-Null
$taskSha = '1234567890abcdef1234567890abcdef12345678'
$taskVersion = '0.1.2'
. (Join-Path $PSScriptRoot 'release-targets.ps1')
$taskNames = @(Get-ReleaseAssetNames -Version $taskVersion)
foreach ($taskName in $taskNames) { [IO.File]::WriteAllText((Join-Path $taskDirectory $taskName), 'fixture ' + $taskName) }
$taskVerifier = Join-Path $PSScriptRoot 'release-assets.ps1'
& $taskVerifier -Mode Prepare -Directory $taskDirectory -Version $taskVersion -SourceSha $taskSha
& $taskVerifier -Mode Verify -Directory $taskDirectory -Version $taskVersion -SourceSha $taskSha

$taskRejected = $false
try { & $taskVerifier -Mode Verify -Directory $taskDirectory -Version $taskVersion -SourceSha ('a' * 40) } catch { $taskRejected = $true }
if (!$taskRejected) { throw 'Verification accepted an unrelated source commit.' }
[IO.File]::AppendAllText((Join-Path $taskDirectory $taskNames[0]), 'tampered')
$taskRejected = $false
try { & $taskVerifier -Mode Verify -Directory $taskDirectory -Version $taskVersion -SourceSha $taskSha } catch { $taskRejected = $true }
if (!$taskRejected) { throw 'Verification accepted modified installer bytes.' }
[IO.File]::WriteAllText((Join-Path $taskDirectory $taskNames[0]), 'fixture ' + $taskNames[0])
# Every architecture and format is mandatory, including both ARM builds.
foreach ($taskName in $taskNames) {
    Move-Item -LiteralPath (Join-Path $taskDirectory $taskName) -Destination (Join-Path $taskDirectory 'wrong-platform.bin')
    $taskRejected = $false
    try { & $taskVerifier -Mode Verify -Directory $taskDirectory -Version $taskVersion -SourceSha $taskSha } catch { $taskRejected = $true }
    if (!$taskRejected) { throw "Verification accepted missing package $taskName." }
    Move-Item -LiteralPath (Join-Path $taskDirectory 'wrong-platform.bin') -Destination (Join-Path $taskDirectory $taskName)
}
Write-Output 'Release verification rejected an incorrect commit, tampering, and a missing platform.'
