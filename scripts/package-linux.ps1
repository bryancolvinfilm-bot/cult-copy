param([string]$Binary = '', [ValidateSet('x86_64','aarch64')][string]$Architecture = 'x86_64')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskVersion = Get-AppVersion -Root $taskRoot
if (!$Binary) { $Binary = Join-Path $taskRoot 'target\x86_64-unknown-linux-gnu\release\duet-clipboard' }
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$taskHeader = [System.IO.File]::ReadAllBytes($Binary)
if ($taskHeader[0] -ne 127 -or [System.Text.Encoding]::ASCII.GetString($taskHeader,1,3) -ne 'ELF') { throw 'The input must be a Linux ELF executable.' }
$taskPayload = Join-Path $taskRoot 'dist\linux-payload'
New-Item -ItemType Directory -Path $taskPayload -Force | Out-Null
Copy-Item -LiteralPath $Binary -Destination (Join-Path $taskPayload 'duet-clipboard') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\duet-clipboard.svg') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'LICENSE') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'NOTICE.md') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'THIRD_PARTY_NOTICES.txt') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\RUST-STDLIB-NOTICES.html') -Destination $taskPayload -Force
$taskArchive = Join-Path $taskRoot 'dist\linux-payload.tar.gz'
& tar -czf $taskArchive -C $taskPayload duet-clipboard duet-clipboard.svg LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html
if ($LASTEXITCODE -ne 0) { throw 'Could not create the Linux payload archive.' }
$taskScript = [System.IO.File]::ReadAllText((Join-Path $taskRoot 'packaging\linux-installer.sh')).Replace("`r`n", "`n")
if (!$taskScript.EndsWith("`n")) { $taskScript += "`n" }
$taskOutput = Join-Path $taskRoot ('dist\DuetClipboard-' + $taskVersion + '-linux-' + $Architecture + '.run')
$taskStream = [System.IO.File]::Create($taskOutput)
try {
    $taskBytes = [System.Text.Encoding]::UTF8.GetBytes($taskScript)
    $taskStream.Write($taskBytes, 0, $taskBytes.Length)
    $taskArchiveStream = [System.IO.File]::OpenRead($taskArchive)
    try { $taskArchiveStream.CopyTo($taskStream) } finally { $taskArchiveStream.Dispose() }
} finally { $taskStream.Dispose() }
Write-Output ('Linux installer: ' + $taskOutput)
