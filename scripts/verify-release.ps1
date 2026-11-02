param([string]$Directory='dist')
$ErrorActionPreference='Stop'
$taskRoot=(Resolve-Path -LiteralPath $Directory).Path
$taskManifest=Get-Content -LiteralPath (Join-Path $taskRoot 'manifest.json') -Raw | ConvertFrom-Json
foreach($taskEntry in $taskManifest){
    if([IO.Path]::GetFileName($taskEntry.name) -ne $taskEntry.name){throw 'Invalid manifest path'}
    $taskFile=Join-Path $taskRoot $taskEntry.name
    if((Get-Item -LiteralPath $taskFile).Length -ne $taskEntry.bytes){throw "Size mismatch: $($taskEntry.name)"}
    if((Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskEntry.sha256){throw "Hash mismatch: $($taskEntry.name)"}
}
$taskVersion=(Get-Item -LiteralPath (Join-Path $taskRoot 'OmniView.exe')).VersionInfo
if($taskVersion.CompanyName -ne 'OmniCONTROL' -or $taskVersion.ProductName -ne 'OmniView'){throw 'Incorrect EXE metadata'}
Write-Output "Hashes and EXE metadata verified: $taskRoot"
