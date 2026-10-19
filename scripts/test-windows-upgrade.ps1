param([Parameter(Mandatory)][ValidateSet('x64','arm64')][string]$Architecture)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Install upgrades only on the disposable GitHub runner.' }
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskVersion = Get-AppVersion -Root $taskRoot
$taskDirectory = Join-Path $env:RUNNER_TEMP ('cult-copy-upgrade-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskDirectory | Out-Null
$taskLegacy = Join-Path $taskDirectory 'legacy-setup.exe'
Invoke-WebRequest -Uri "https://github.com/bryancolvinfilm-bot/cult-copy/releases/download/v0.1.5/DuetClipboard-0.1.5-windows-$Architecture-setup.exe" -OutFile $taskLegacy
$taskInstall = Join-Path $taskDirectory 'installed'
$taskKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{736A9FD9-50C7-43B8-9E1C-A9AA51AF943E}_is1'
if (Test-Path $taskKey) { throw 'The runner already has an installation.' }
$taskProcess = Start-Process $taskLegacy -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',('/DIR="' + $taskInstall + '"')) -PassThru -Wait
if ($taskProcess.ExitCode -ne 0) { throw 'Legacy installation failed.' }
if (!(Get-ItemProperty $taskKey).DisplayName.StartsWith('Duet Clipboard')) { throw 'Missing legacy uninstall registration.' }
# Omit /DIR on upgrade: the preserved AppId must recover the previous location.
$taskSetup = Join-Path $taskRoot "dist\CultCopy-$taskVersion-windows-$Architecture-setup.exe"
$taskProcess = Start-Process $taskSetup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -PassThru -Wait
if ($taskProcess.ExitCode -ne 0) { throw 'Cult Copy upgrade failed.' }
$taskRegistration = Get-ItemProperty $taskKey
if ($taskRegistration.DisplayName -ne 'Cult Copy' -or $taskRegistration.DisplayVersion -ne $taskVersion) { throw 'Upgrade did not reuse the uninstall registration.' }
if ($taskRegistration.InstallLocation.TrimEnd('\') -ne $taskInstall) { throw 'Upgrade changed the existing installation directory.' }
if ((Get-ChildItem -LiteralPath $taskInstall -Filter 'unins*.exe').Count -ne 1) { throw 'Upgrade created a second uninstaller.' }
$taskPrograms = [Environment]::GetFolderPath('Programs')
if (Test-Path (Join-Path $taskPrograms 'Duet Clipboard.lnk')) { throw 'Legacy shortcut remains.' }
if (!(Test-Path (Join-Path $taskPrograms 'Cult Copy.lnk'))) { throw 'Missing Cult Copy shortcut.' }
Write-Output 'Windows upgraded the original installer to Cult Copy in place with one uninstall entry and the new shortcut.'
