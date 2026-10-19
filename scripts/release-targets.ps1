function Get-ReleaseAssetNames {
    param([Parameter(Mandatory=$true)][string]$Version)
    # Published assets keep their immutable filenames across the repository rename.
    $taskPrefix = if ([version]$Version -lt [version]'0.1.6') { 'DuetClipboard' } else { 'CultCopy' }
    $taskTargets = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../packaging/release-targets.json') -Raw | ConvertFrom-Json
    foreach ($taskTarget in $taskTargets) {
        foreach ($taskPackage in $taskTarget.packages) {
            if ($taskPackage -notmatch '^(?:windows-(?:x64|arm64)-setup\.exe|macos-universal\.(?:dmg|pkg)|linux-(?:x64|arm64)\.(?:AppImage|run)|(?:fedora|rocky8)-(?:x64|arm64)\.rpm)$') { throw 'Invalid release package filename.' }
            "$taskPrefix-$Version-$taskPackage"
        }
    }
}
