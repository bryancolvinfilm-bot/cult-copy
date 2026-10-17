function Get-AppVersion {
    param([string]$Root = (Split-Path -Parent $PSScriptRoot))
    $taskManifest = [IO.File]::ReadAllText((Join-Path $Root 'Cargo.toml'))
    $taskMatch = [regex]::Match($taskManifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"\s*$')
    if (!$taskMatch.Success) { throw 'Cargo.toml must contain a stable major.minor.patch package version.' }
    return $taskMatch.Groups[1].Value
}
