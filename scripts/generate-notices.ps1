$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
if (Test-Path -LiteralPath (Join-Path $taskRoot '.tools\cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $taskRoot '.tools\cargo'
    $env:RUSTUP_HOME = Join-Path $taskRoot '.tools\rustup'
    $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
    $taskLlvm = Get-ChildItem (Join-Path $taskRoot '.tools') -Directory -Filter 'llvm-mingw-*' | Select-Object -First 1
    if ($taskLlvm) { $env:PATH = (Join-Path $taskLlvm.FullName 'bin') + ';' + $env:PATH }
}
Push-Location -LiteralPath $taskRoot
try {
    $taskMetadataText = & cargo metadata --locked --offline --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Could not read the locked dependency metadata.' }
    $taskMetadata = ($taskMetadataText -join "`n") | ConvertFrom-Json
    $taskText = [System.Text.StringBuilder]::new()
    [void]$taskText.AppendLine('CULT COPY - THIRD-PARTY NOTICES')
    [void]$taskText.AppendLine('Dependency license texts from the locked source packages, including platform-specific build dependencies.')
    $taskLicenses = [ordered]@{}
    foreach ($taskPackage in ($taskMetadata.packages | Where-Object { $_.source } | Sort-Object name,version)) {
        [void]$taskText.AppendLine("`n" + $taskPackage.name + ' ' + $taskPackage.version + ' | ' + $taskPackage.license)
        if ($taskPackage.repository) { [void]$taskText.AppendLine($taskPackage.repository) }
        $taskFiles = Get-ChildItem -LiteralPath (Split-Path -Parent $taskPackage.manifest_path) -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)([.\-_]|$)' }
        foreach ($taskFile in $taskFiles) {
            $taskHash = (Get-FileHash -LiteralPath $taskFile.FullName -Algorithm SHA256).Hash
            [void]$taskText.AppendLine('  ' + $taskFile.Name + ': license-' + $taskHash)
            if (!$taskLicenses.Contains($taskHash)) { $taskLicenses[$taskHash] = [System.IO.File]::ReadAllText($taskFile.FullName) }
        }
    }
    foreach ($taskHash in $taskLicenses.Keys) {
        [void]$taskText.AppendLine("`n================ license-" + $taskHash + " ================`n")
        [void]$taskText.AppendLine($taskLicenses[$taskHash])
    }
    [System.IO.File]::WriteAllText((Join-Path $taskRoot 'THIRD_PARTY_NOTICES.txt'), $taskText.ToString(), [System.Text.UTF8Encoding]::new($false))
    Write-Output ('Generated notices for ' + $taskMetadata.packages.Count + ' packages with ' + $taskLicenses.Count + ' distinct license texts.')
} finally { Pop-Location }
