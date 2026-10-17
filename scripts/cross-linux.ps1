param([Parameter(Mandatory = $true)][string]$Zig)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$Zig = (Resolve-Path -LiteralPath $Zig).Path
$taskTools = Join-Path $taskRoot '.tools'
New-Item -ItemType Directory -Path $taskTools -Force | Out-Null
$taskLinker = Join-Path $taskTools 'zig-linux-link.cmd'
$taskArchiver = Join-Path $taskTools 'zig-linux-ar.cmd'
# Zig uses a shorter target spelling than Rust. Put it last to override the
# --target argument that cc-rs adds automatically for a Clang-like compiler.
Set-Content -LiteralPath $taskLinker -Value ('@"' + $Zig + '" cc %* -target x86_64-linux-gnu.2.28') -Encoding ascii
Set-Content -LiteralPath $taskArchiver -Value ('@"' + $Zig + '" ar %*') -Encoding ascii
$env:CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = $taskLinker
$env:CC_x86_64_unknown_linux_gnu = $taskLinker
$env:AR_x86_64_unknown_linux_gnu = $taskArchiver
$env:ZIG_GLOBAL_CACHE_DIR = Join-Path $taskTools 'zig-global-cache'
$env:ZIG_LOCAL_CACHE_DIR = Join-Path $taskTools 'zig-cache'
& (Join-Path $PSScriptRoot 'dev.ps1') -CargoArgs @('build','--release','--target','x86_64-unknown-linux-gnu','--locked')
if ($LASTEXITCODE -ne 0) { throw 'Linux cross-compilation failed. Ensure the Linux Rust target is installed.' }
& (Join-Path $PSScriptRoot 'package-linux.ps1')
