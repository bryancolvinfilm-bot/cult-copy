function Get-ReleaseAssetNames {
    param([Parameter(Mandatory=$true)][string]$Version)
    $taskTargets = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../packaging/release-targets.json') -Raw | ConvertFrom-Json
    foreach ($taskTarget in $taskTargets) {
        foreach ($taskPackage in $taskTarget.packages) {
            if ($taskPackage -notmatch '^(?:windows-(?:x64|arm64)-setup\.exe|macos-universal\.(?:dmg|pkg)|linux-(?:x64|arm64)\.(?:AppImage|run)|(?:fedora|rocky8)-(?:x64|arm64)\.rpm)$') { throw 'Invalid release package filename.' }
            "DuetClipboard-$Version-$taskPackage"
        }
    }
}
