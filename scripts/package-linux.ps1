param([string]$Binary = '', [ValidateSet('x64','arm64')][string]$Architecture = 'x64')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskVersion = Get-AppVersion -Root $taskRoot
if (!$Binary) {
    $taskTarget = if ($Architecture -eq 'arm64') { 'aarch64-unknown-linux-gnu' } else { 'x86_64-unknown-linux-gnu' }
    $Binary = Join-Path $taskRoot ('target\' + $taskTarget + '\release\cult-copy')
}
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$taskHeader = [System.IO.File]::ReadAllBytes($Binary)
if ($taskHeader[0] -ne 127 -or [System.Text.Encoding]::ASCII.GetString($taskHeader,1,3) -ne 'ELF') { throw 'The input must be a Linux ELF executable.' }
$taskMachine = [BitConverter]::ToUInt16($taskHeader,18)
$taskExpectedMachine = if ($Architecture -eq 'arm64') { 183 } else { 62 }
if ($taskMachine -ne $taskExpectedMachine) { throw 'The ELF architecture does not match the requested installer.' }
$taskPayload = Join-Path $taskRoot 'dist\linux-payload'
New-Item -ItemType Directory -Path $taskPayload -Force | Out-Null
Copy-Item -LiteralPath $Binary -Destination (Join-Path $taskPayload 'cult-copy') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\cult-copy.svg') -Destination (Join-Path $taskPayload 'cult-copy.svg') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\cult-copy.desktop') -Destination (Join-Path $taskPayload 'cult-copy.desktop') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\icons\icon-256.png') -Destination (Join-Path $taskPayload 'cult-copy.png') -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'LICENSE') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'NOTICE.md') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'THIRD_PARTY_NOTICES.txt') -Destination $taskPayload -Force
Copy-Item -LiteralPath (Join-Path $taskRoot 'packaging\RUST-STDLIB-NOTICES.html') -Destination $taskPayload -Force
$taskArchive = Join-Path $taskRoot 'dist\linux-payload.tar.gz'
& tar -czf $taskArchive -C $taskPayload cult-copy cult-copy.svg cult-copy.png cult-copy.desktop io.github.bryancolvinfilm_bot.CultCopy.metainfo.xml LICENSE NOTICE.md THIRD_PARTY_NOTICES.txt RUST-STDLIB-NOTICES.html
if ($LASTEXITCODE -ne 0) { throw 'Could not create the Linux payload archive.' }
$taskScript = [System.IO.File]::ReadAllText((Join-Path $taskRoot 'packaging\linux-installer.sh')).Replace("`r`n", "`n")
if (!$taskScript.EndsWith("`n")) { $taskScript += "`n" }
$taskOutput = Join-Path $taskRoot ('dist\CultCopy-' + $taskVersion + '-linux-' + $Architecture + '.run')
$taskStream = [System.IO.File]::Create($taskOutput)
try {
    $taskBytes = [System.Text.Encoding]::UTF8.GetBytes($taskScript)
    $taskStream.Write($taskBytes, 0, $taskBytes.Length)
    $taskArchiveStream = [System.IO.File]::OpenRead($taskArchive)
    try { $taskArchiveStream.CopyTo($taskStream) } finally { $taskArchiveStream.Dispose() }
} finally { $taskStream.Dispose() }
Write-Output ('Linux installer: ' + $taskOutput)
