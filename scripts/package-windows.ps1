param([string]$Binary = '', [string]$Compiler = '', [ValidateSet('auto','x64','arm64')][string]$Architecture = 'auto')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskVersion = Get-AppVersion -Root $taskRoot
if (!$Binary) { $Binary = Join-Path $taskRoot 'target\release\duet-clipboard.exe' }
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$taskPe = [IO.File]::ReadAllBytes($Binary)
$taskMachine = [BitConverter]::ToUInt16($taskPe, [BitConverter]::ToInt32($taskPe, 60) + 4)
$taskDetected = switch ($taskMachine) { 0x8664 { 'x64' }; 0xaa64 { 'arm64' }; default { throw 'Expected a Windows x64 or ARM64 PE executable.' } }
if ($Architecture -eq 'auto') { $Architecture = $taskDetected }
if ($Architecture -ne $taskDetected) { throw 'The executable architecture does not match the requested installer.' }
if (!$Compiler) {
    $taskCandidates = @(
        (Join-Path $taskRoot '.tools\InnoSetup\ISCC.exe'),
        'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
        'C:\Program Files\Inno Setup 7\ISCC.exe',
        'C:\Program Files (x86)\Inno Setup 7\ISCC.exe'
    )
    $Compiler = $taskCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if (!$Compiler) { throw 'Install Inno Setup, or provide -Compiler with the path to ISCC.exe.' }
$taskRuntime = Join-Path $taskRoot ('dist\runtime-' + $Architecture)
New-Item -ItemType Directory -Path $taskRuntime -Force | Out-Null
$taskLlvm = Get-ChildItem -LiteralPath (Join-Path $taskRoot '.tools') -Directory -Filter 'llvm-mingw-*' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($taskLlvm) {
    $taskObjdump = Join-Path $taskLlvm.FullName 'bin\llvm-objdump.exe'
    $taskNeedsUnwind = & $taskObjdump -p $Binary | Select-String 'DLL Name:\s+libunwind.dll'
    if ($taskNeedsUnwind) {
        if ($Architecture -ne 'x64') { throw 'The local LLVM runtime bundle is x64 only; build ARM64 with native MSVC.' }
        Copy-Item -LiteralPath (Join-Path $taskLlvm.FullName 'bin\libunwind.dll') -Destination (Join-Path $taskRuntime 'libunwind.dll') -Force
        Copy-Item -LiteralPath (Join-Path $taskLlvm.FullName 'LICENSE.TXT') -Destination (Join-Path $taskRuntime 'LICENSE-LLVM.txt') -Force
    }
}
& $Compiler ('/DAppBinary=' + $Binary) ('/DAppVersion=' + $taskVersion) ('/DAppArchitecture=' + $Architecture) ('/DAppRuntime=' + $taskRuntime) (Join-Path $taskRoot 'packaging\windows.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
