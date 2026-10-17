param([int]$TimeoutMinutes = 90)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'version.ps1')
$taskGit = (Get-Command git -ErrorAction SilentlyContinue).Source
if (!$taskGit -and (Test-Path -LiteralPath 'C:\Program Files\Git\cmd\git.exe')) { $taskGit = 'C:\Program Files\Git\cmd\git.exe' }
if (!$taskGit) { throw 'Git is required.' }
function Invoke-ProjectGit {
    param([string[]]$Arguments)
    $taskResult = & $taskGit -C $taskRoot @Arguments
    if ($LASTEXITCODE -ne 0) { throw ('Git failed: ' + ($Arguments -join ' ')) }
    return $taskResult
}
if (@(Invoke-ProjectGit -Arguments @('status','--porcelain')).Count -ne 0) { throw 'Review, stage, and commit the intended changes first. The publisher requires a clean working tree.' }
if ((Invoke-ProjectGit -Arguments @('branch','--show-current')) -ne 'main') { throw 'Publish from main after integrating the intended changes.' }
$taskOrigin = Invoke-ProjectGit -Arguments @('remote','get-url','origin')
if ($taskOrigin -notmatch '^(?:https://github\.com/|git@github\.com:)([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(?:\.git)?$') { throw 'origin must be a GitHub repository URL without embedded credentials.' }
$taskRepository = $Matches[1]
$env:GIT_TERMINAL_PROMPT = '0'
$env:GCM_INTERACTIVE = 'Never'

# Use existing authentication. Never write a token to disk, a Git remote, or output.
$taskToken = $env:GH_TOKEN
if (!$taskToken) { $taskToken = $env:GITHUB_TOKEN }
if (!$taskToken -and (Get-Command gh -ErrorAction SilentlyContinue)) {
    $taskToken = & gh auth token 2>$null
    if ($LASTEXITCODE -ne 0) { $taskToken = $null }
}
if (!$taskToken) {
    $taskInput = "protocol=https`nhost=github.com`n`n"
    $taskCredentials = $taskInput | & $taskGit -c credential.interactive=never credential fill
    if ($LASTEXITCODE -ne 0) { throw 'Sign into GitHub with Git Credential Manager or GitHub CLI before publishing.' }
    foreach ($taskLine in $taskCredentials) {
        if ($taskLine.StartsWith('password=')) { $taskToken = $taskLine.Substring(9) }
    }
    $taskCredentials = $null
}
if (!$taskToken) { throw 'No GitHub authentication is available.' }
$taskHeaders = @{Authorization=('Bearer '+$taskToken);Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28';'User-Agent'='DuetClipboard-publisher'}
$taskToken = $null
function Invoke-ProjectApi {
    param([string]$Path, [string]$Method = 'Get', [object]$Body = $null, [switch]$AllowNotFound)
    $taskRequest = @{Uri=('https://api.github.com/repos/'+$taskRepository+'/'+$Path);Headers=$taskHeaders;Method=$Method}
    if ($null -ne $Body) { $taskRequest.ContentType='application/json'; $taskRequest.Body=$Body | ConvertTo-Json -Depth 8 -Compress }
    try { return Invoke-RestMethod @taskRequest }
    catch {
        $taskStatus = $_.Exception.Response.StatusCode.value__
        if ($AllowNotFound -and $taskStatus -eq 404) { return $null }
        throw "GitHub request failed (HTTP $taskStatus): $Method $Path"
    }
}

Invoke-ProjectGit -Arguments @('fetch','origin','main','--tags') | Out-Host
$taskCounts = (Invoke-ProjectGit -Arguments @('rev-list','--left-right','--count','HEAD...origin/main')) -split '\s+'
if ([int]$taskCounts[1] -gt 0) {
    if ([int]$taskCounts[0] -gt 0) { throw 'main has diverged from origin/main. Resolve it without force-pushing, then retry.' }
    Invoke-ProjectGit -Arguments @('merge','--ff-only','origin/main') | Out-Host
}
$taskSha = Invoke-ProjectGit -Arguments @('rev-parse','HEAD')
$taskVersion = Get-AppVersion -Root $taskRoot
$taskTag = 'v' + $taskVersion
$taskExistingTags = @(Invoke-ProjectGit -Arguments @('tag','--list',$taskTag))
if ($taskExistingTags.Count -gt 0 -and (Invoke-ProjectGit -Arguments @('rev-list','-n','1',$taskTag)) -ne $taskSha) { throw 'The version tag belongs to another commit. Bump Cargo.toml and regenerate Cargo.lock before publishing new code.' }
$taskExistingRelease = Invoke-ProjectApi -Path "releases/tags/$taskTag" -AllowNotFound
Invoke-ProjectGit -Arguments @('push','origin','main') | Out-Host
if ((Invoke-ProjectApi -Path 'git/ref/heads/main').object.sha -ne $taskSha) { throw 'Remote main does not match the local commit after push.' }
Write-Host "Source synchronized: $taskSha ($taskTag)"
$taskDeadline = [DateTime]::UtcNow.AddMinutes($TimeoutMinutes)

function Get-ProjectRuns {
    return @((Invoke-ProjectApi -Path 'actions/workflows/build.yml/runs?branch=main&per_page=50').workflow_runs | Where-Object { $_.head_sha -eq $taskSha })
}
function Start-ProjectRun {
    param([bool]$Publish)
    $taskBeforeIds = @(Get-ProjectRuns | Select-Object -ExpandProperty id)
    $null = Invoke-ProjectApi -Path 'actions/workflows/build.yml/dispatches' -Method Post -Body @{ref='main';inputs=@{expected_sha=$taskSha;publish_release=$Publish}}
    $taskLabel = if ($Publish) { 'Publish ' } else { 'Check ' }
    while ([DateTime]::UtcNow -lt $taskDeadline) {
        $taskNew = @(Get-ProjectRuns | Where-Object { $_.id -notin $taskBeforeIds -and $_.event -eq 'workflow_dispatch' -and $_.display_title.StartsWith($taskLabel) })
        if ($taskNew.Count -gt 0) { return $taskNew[0] }
        Start-Sleep -Seconds 5
    }
    throw 'Timed out locating the dispatched workflow. See the repository Actions page.'
}
function Wait-ProjectRun {
    param([object]$Run)
    Write-Host ('Watching ' + $Run.html_url)
    $taskLastState = ''
    while ([DateTime]::UtcNow -lt $taskDeadline) {
        $taskCurrent = Invoke-ProjectApi -Path ('actions/runs/' + $Run.id)
        if ($taskCurrent.head_sha -ne $taskSha) { throw 'The workflow is building a different source commit.' }
        $taskJobs = (Invoke-ProjectApi -Path ('actions/runs/' + $Run.id + '/jobs')).jobs
        $taskState = (@($taskJobs | ForEach-Object { $_.name + ': ' + $(if ($_.conclusion) { $_.conclusion } else { $_.status }) }) -join '; ')
        if ($taskState -ne $taskLastState) { Write-Host $taskState; $taskLastState = $taskState }
        if ($taskCurrent.status -eq 'completed') {
            if ($taskCurrent.conclusion -ne 'success') { throw ('Workflow '+$taskCurrent.conclusion+': '+$taskCurrent.html_url+'. Inspect failed logs, fix, commit, push, and rerun; do not publish partial builds.') }
            return $taskCurrent
        }
        Start-Sleep -Seconds 10
    }
    throw ('Timed out waiting. The workflow remains available at ' + $Run.html_url)
}

if (!$taskExistingRelease -or $taskExistingRelease.draft) {
    # A push-triggered run can take a few seconds to appear in the API. Allow
    # that registration delay before dispatching a replacement check ourselves.
    $taskCheckRuns = @()
    for ($taskAttempt = 0; $taskAttempt -lt 6 -and $taskCheckRuns.Count -eq 0; $taskAttempt++) {
        $taskCheckRuns = @(Get-ProjectRuns | Where-Object { $_.display_title.StartsWith('Check ') })
        if ($taskCheckRuns.Count -eq 0) { Start-Sleep -Seconds 5 }
    }
    $taskCheck = if ($taskCheckRuns.Count -gt 0) { $taskCheckRuns[0] } else { Start-ProjectRun -Publish $false }
    $null = Wait-ProjectRun -Run $taskCheck
    if ((Invoke-ProjectApi -Path 'git/ref/heads/main').object.sha -ne $taskSha) { throw 'main changed after checks; synchronize and retry.' }
    $taskPublishRuns = @(Get-ProjectRuns | Where-Object { $_.display_title.StartsWith('Publish ') -and $_.status -ne 'completed' })
    $taskPublish = if ($taskPublishRuns.Count -gt 0) { $taskPublishRuns[0] } else { Start-ProjectRun -Publish $true }
    $null = Wait-ProjectRun -Run $taskPublish
}
$taskRelease = Invoke-ProjectApi -Path "releases/tags/$taskTag"
if ($taskRelease.draft -or $taskRelease.prerelease) { throw 'The release is not a public latest release.' }
$taskNames = @("DuetClipboard-$taskVersion-windows-x64-setup.exe", "DuetClipboard-$taskVersion-macos-universal.pkg", "DuetClipboard-$taskVersion-linux-x86_64.run", 'CHECKSUMS.sha256', 'BUILD-INFO.json')
if (@(Compare-Object $taskNames @($taskRelease.assets.name)).Count -ne 0) { throw 'Published release assets are incomplete or unexpected.' }
$taskDownloads = Join-Path $taskRoot ('.tools\public-download-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskDownloads -Force | Out-Null
foreach ($taskAsset in $taskRelease.assets) {
    $taskUrlPrefix = 'https://github.com/' + $taskRepository + '/releases/download/' + $taskTag + '/'
    if (!$taskAsset.browser_download_url.StartsWith($taskUrlPrefix)) { throw 'Unexpected public download URL.' }
    Invoke-WebRequest -UseBasicParsing -Uri $taskAsset.browser_download_url -OutFile (Join-Path $taskDownloads $taskAsset.name)
}
& (Join-Path $PSScriptRoot 'release-assets.ps1') -Mode Verify -Directory $taskDownloads -Version $taskVersion -SourceSha $taskSha -Repository $taskRepository
if (@(Invoke-ProjectGit -Arguments @('status','--porcelain')).Count -ne 0) { throw 'The working tree changed while publishing. Review changes before reporting completion.' }
Write-Host ('Public release: ' + $taskRelease.html_url)
foreach ($taskAsset in $taskRelease.assets) { Write-Host $taskAsset.browser_download_url }
Write-Host ('Anonymous downloads verified in ' + $taskDownloads)
