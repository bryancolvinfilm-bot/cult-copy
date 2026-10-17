param([string]$Binary = '', [string]$Compiler = '')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskVersion = Get-AppVersion -Root $taskRoot
if (!$Binary) { $Binary = Join-Path $taskRoot 'target\release\duet-clipboard.exe' }
$Binary = (Resolve-Path -LiteralPath $Binary).Path
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
New-Item -ItemType Directory -Path (Join-Path $taskRoot 'dist\runtime') -Force | Out-Null
$taskLlvm = Get-ChildItem -LiteralPath (Join-Path $taskRoot '.tools') -Directory -Filter 'llvm-mingw-*' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($taskLlvm) {
    $taskObjdump = Join-Path $taskLlvm.FullName 'bin\llvm-objdump.exe'
    $taskNeedsUnwind = & $taskObjdump -p $Binary | Select-String 'DLL Name:\s+libunwind.dll'
    if ($taskNeedsUnwind) {
        Copy-Item -LiteralPath (Join-Path $taskLlvm.FullName 'bin\libunwind.dll') -Destination (Join-Path $taskRoot 'dist\runtime\libunwind.dll') -Force
        Copy-Item -LiteralPath (Join-Path $taskLlvm.FullName 'LICENSE.TXT') -Destination (Join-Path $taskRoot 'dist\runtime\LICENSE-LLVM.txt') -Force
    }
}
& $Compiler ('/DAppBinary=' + $Binary) ('/DAppVersion=' + $taskVersion) (Join-Path $taskRoot 'packaging\windows.iss')
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed.' }
