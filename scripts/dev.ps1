param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArgs = @('run'))
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
if (Test-Path -LiteralPath (Join-Path $taskRoot '.tools\cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $taskRoot '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $taskRoot '.tools\rustup'
    $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
    $taskLinker = Get-ChildItem -LiteralPath (Join-Path $taskRoot '.tools') -Directory -Filter 'llvm-mingw-*' | Select-Object -First 1
    if ($taskLinker) {
        $env:PATH = (Join-Path $taskLinker.FullName 'bin') + ';' + $env:PATH
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $taskLinker.FullName 'bin\x86_64-w64-mingw32-clang.exe'
        $env:CC_x86_64_pc_windows_gnullvm = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
        $env:AR_x86_64_pc_windows_gnullvm = Join-Path $taskLinker.FullName 'bin\llvm-ar.exe'
    }
}
Push-Location -LiteralPath $taskRoot
try { & cargo @CargoArgs; $taskResult = $LASTEXITCODE } finally { Pop-Location }
exit $taskResult
