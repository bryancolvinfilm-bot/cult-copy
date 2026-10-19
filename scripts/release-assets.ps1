param(
    [Parameter(Mandatory=$true)][ValidateSet('Prepare','Verify')][string]$Mode,
    [Parameter(Mandatory=$true)][string]$Directory,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')][string]$Version,
    [Parameter(Mandatory=$true)][ValidatePattern('^[a-f0-9]{40}$')][string]$SourceSha,
    [string]$Repository = 'bryancolvinfilm-bot/cult-copy',
    [string]$RunUrl = ''
)
$ErrorActionPreference = 'Stop'
$taskDirectory = (Resolve-Path -LiteralPath $Directory).Path
. (Join-Path $PSScriptRoot 'release-targets.ps1')
$taskNames = @(Get-ReleaseAssetNames -Version $Version)
$taskManifestPath = Join-Path $taskDirectory 'BUILD-INFO.json'
$taskChecksumsPath = Join-Path $taskDirectory 'CHECKSUMS.sha256'
$taskUtf8 = [Text.UTF8Encoding]::new($false)
foreach ($taskName in $taskNames) {
    $taskFile = Get-Item -LiteralPath (Join-Path $taskDirectory $taskName)
    if ($taskFile.Length -eq 0) { throw "Empty installer: $taskName" }
}
if ($Mode -eq 'Prepare') {
    $taskAssets = foreach ($taskName in $taskNames) {
        $taskPath = Join-Path $taskDirectory $taskName
        [ordered]@{
            name = $taskName
            bytes = (Get-Item -LiteralPath $taskPath).Length
            sha256 = (Get-FileHash -LiteralPath $taskPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $taskManifest = [ordered]@{
        version = $Version
        source_commit = $SourceSha
        repository = $Repository
        workflow_run = $RunUrl
        assets = @($taskAssets)
    }
    [IO.File]::WriteAllText($taskManifestPath, ($taskManifest | ConvertTo-Json -Depth 5) + "`n", $taskUtf8)
    $taskLines = foreach ($taskName in @($taskNames) + @('BUILD-INFO.json')) {
        (Get-FileHash -LiteralPath (Join-Path $taskDirectory $taskName) -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + $taskName
    }
    [IO.File]::WriteAllText($taskChecksumsPath, ($taskLines -join "`n") + "`n", $taskUtf8)
}
# Resolve only the fixed expected filenames, never paths supplied by a manifest.
$taskExpectedNames = @($taskNames) + @('BUILD-INFO.json','CHECKSUMS.sha256')
$taskActualNames = @(Get-ChildItem -LiteralPath $taskDirectory -Force | Select-Object -ExpandProperty Name)
if (@(Compare-Object $taskExpectedNames $taskActualNames).Count -ne 0) { throw 'Missing or unexpected files in the release directory.' }
$taskManifest = Get-Content -LiteralPath $taskManifestPath -Raw | ConvertFrom-Json
if ($taskManifest.version -ne $Version -or $taskManifest.source_commit -ne $SourceSha -or $taskManifest.repository -ne $Repository) {
    throw 'Build metadata does not match the requested version, repository, and source commit.'
}
if (@($taskManifest.assets).Count -ne $taskNames.Count -or @(Compare-Object $taskNames @($taskManifest.assets.name)).Count -ne 0) { throw 'Invalid installer list in build metadata.' }
$taskLines = @(Get-Content -LiteralPath $taskChecksumsPath)
if ($taskLines.Count -ne ($taskNames.Count + 1)) { throw 'Expected checksums for every package and BUILD-INFO.json.' }
$taskExpectedHashes = @{}
foreach ($taskLine in $taskLines) {
    if ($taskLine -notmatch '^([a-f0-9]{64})  ([^/\\]+)$') { throw 'Invalid checksum line.' }
    $taskName = $Matches[2]
    if ($taskName -notin (@($taskNames) + @('BUILD-INFO.json')) -or $taskExpectedHashes.ContainsKey($taskName)) { throw 'Unexpected or duplicate checksum filename.' }
    $taskExpectedHashes[$taskName] = $Matches[1]
}
foreach ($taskName in @($taskNames) + @('BUILD-INFO.json')) {
    $taskPath = Join-Path $taskDirectory $taskName
    $taskHash = (Get-FileHash -LiteralPath $taskPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($taskHash -ne $taskExpectedHashes[$taskName]) { throw "Checksum mismatch: $taskName" }
    if ($taskName -in $taskNames) {
        $taskEntry = @($taskManifest.assets | Where-Object { $_.name -eq $taskName })[0]
        if ($taskEntry.sha256 -ne $taskHash -or $taskEntry.bytes -ne (Get-Item -LiteralPath $taskPath).Length) { throw "Installer metadata mismatch: $taskName" }
    }
}
Write-Output "Verified all $($taskNames.Count) packages and checksums for v$Version at $SourceSha."
