function Get-ReleaseAssetNames {
    param([Parameter(Mandatory=$true)][string]$Version)
    $taskTargets = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../packaging/release-targets.json') -Raw | ConvertFrom-Json
    foreach ($taskTarget in $taskTargets) {
        foreach ($taskPackage in $taskTarget.packages) {
            if ($taskPackage -notmatch '^(windows|macos|linux)-[a-z0-9]+(?:-setup)?\.(exe|dmg|pkg|AppImage|run)$') { throw 'Invalid release package filename.' }
            "DuetClipboard-$Version-$taskPackage"
        }
    }
}
