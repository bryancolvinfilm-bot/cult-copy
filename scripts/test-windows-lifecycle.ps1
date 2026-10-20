param([string]$Binary = '')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
if (!$Binary) { $Binary = Join-Path $taskRoot 'target\release\cult-copy.exe' }
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$taskProfile = Join-Path $taskRoot ('.tools\lifecycle-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskProfile -Force | Out-Null
$taskReport = Join-Path $taskProfile 'result.json'
$taskPreviousReport = $env:CULT_COPY_LIFECYCLE_TEST_TO
try {
    $env:CULT_COPY_LIFECYCLE_TEST_TO = $taskReport
    $taskProcess = Start-Process -FilePath $Binary -WindowStyle Hidden -ArgumentList @('--data-dir', ('"' + $taskProfile + '"')) -PassThru
    if (!$taskProcess.WaitForExit(20000)) {
        # Stop only this test process. Keep its isolated profile for diagnosis.
        & taskkill.exe /PID $taskProcess.Id /F | Out-Null
        throw 'The lifecycle test did not exit. Build with --features ui-test before running this script.'
    }
    if (!(Test-Path -LiteralPath $taskReport)) { throw 'Missing lifecycle report; build with --features ui-test.' }
    $taskResult = Get-Content -LiteralPath $taskReport -Raw | ConvertFrom-Json
    Get-Content -LiteralPath $taskReport
    if ($taskProcess.ExitCode -ne 0 -or !$taskResult.passed) { throw 'Windows lifecycle test failed.' }
    Write-Output 'Windows close, hidden refresh, restore, activation, minimize, and quit checks passed.'
} finally {
    if ($null -eq $taskPreviousReport) {
        Remove-Item Env:CULT_COPY_LIFECYCLE_TEST_TO -ErrorAction SilentlyContinue
    } else { $env:CULT_COPY_LIFECYCLE_TEST_TO = $taskPreviousReport }
}
