$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskVersion = Get-AppVersion -Root $taskRoot
$taskDist = Join-Path $taskRoot 'dist'
New-Item -ItemType Directory -Path $taskDist -Force | Out-Null
$taskOutput = Join-Path $taskDist ('CultCopy-' + $taskVersion + '-mac-build-source.zip')
$taskFiles = @(
    'Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE', 'NOTICE.md', 'README.md', 'CONTRIBUTING.md',
    'publish.md', 'AGENTS.md', 'THIRD_PARTY_NOTICES.txt', 'rustfmt.toml', '.gitignore', '.gitattributes'
) | ForEach-Object { Get-Item -LiteralPath (Join-Path $taskRoot $_) }
foreach ($taskDirectory in @('src', 'tests', 'scripts', 'docs', 'packaging', '.cargo', '.github')) {
    $taskFiles += Get-ChildItem -LiteralPath (Join-Path $taskRoot $taskDirectory) -File -Recurse -Force
}
if (Test-Path -LiteralPath $taskOutput) { Remove-Item -LiteralPath $taskOutput -Force }
$taskArchive = [IO.Compression.ZipFile]::Open($taskOutput, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($taskFile in ($taskFiles | Sort-Object FullName -Unique)) {
        $taskRelative = $taskFile.FullName.Substring($taskRoot.Length + 1).Replace('\', '/')
        $taskEntry = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($taskArchive, $taskFile.FullName, ('CultCopy-mac-build/' + $taskRelative), [IO.Compression.CompressionLevel]::Optimal)
        # Unix regular files: executable shell helpers, read/write source files.
        $taskEntry.ExternalAttributes = if ($taskFile.Extension -in '.command', '.sh') { -2115174400 } else { -2119958528 }
    }
    $taskEntry = $taskArchive.CreateEntry('CultCopy-mac-build/START-HERE-MAC.txt')
    $taskWriter = [IO.StreamWriter]::new($taskEntry.Open(), [Text.UTF8Encoding]::new($false))
    try {
        $taskWriter.WriteLine('This is the Mac source/build kit, not an already-built installer.')
        $taskWriter.WriteLine('On your Mac, open scripts/Build Mac App.command.')
        $taskWriter.WriteLine('If it does not open, type bash followed by a space in Terminal, drag that file into Terminal, and press Return.')
        $taskWriter.WriteLine('The helper produces a universal Mac DMG and PKG, and selects the DMG in Finder.')
        $taskWriter.WriteLine('See docs/MAC-BUILD.md for prerequisites and Mac-to-PC pairing instructions.')
    } finally { $taskWriter.Dispose() }
    $taskEntry.ExternalAttributes = -2119958528
} finally { $taskArchive.Dispose() }

# Mark the central-directory entries as Unix so macOS extraction preserves the
# executable mode on .command helpers. ZipArchive otherwise records Windows.
$taskBytes = [IO.File]::ReadAllBytes($taskOutput)
$taskOffset = [BitConverter]::ToUInt32($taskBytes, $taskBytes.Length - 6)
while ([BitConverter]::ToUInt32($taskBytes, $taskOffset) -eq 0x02014b50) {
    $taskBytes[$taskOffset + 5] = 3
    $taskNameLength = [BitConverter]::ToUInt16($taskBytes, $taskOffset + 28)
    $taskExtraLength = [BitConverter]::ToUInt16($taskBytes, $taskOffset + 30)
    $taskCommentLength = [BitConverter]::ToUInt16($taskBytes, $taskOffset + 32)
    $taskOffset += 46 + $taskNameLength + $taskExtraLength + $taskCommentLength
}
[IO.File]::WriteAllBytes($taskOutput, $taskBytes)
Write-Output ('Mac source/build kit: ' + $taskOutput)
