param([string[]]$Architectures=@('x64','x86'),[switch]$IncludeNativeInput)
$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
foreach($taskArch in $Architectures){
    $taskExe=Join-Path $taskRoot $(if($taskArch -eq 'x86'){'dist/win32/OmniView.exe'}else{'dist/OmniView.exe'})
    $taskOut=Join-Path $taskRoot "verification/$taskArch"
    New-Item -ItemType Directory -Path $taskOut -Force | Out-Null
    $taskChecks=@(@{Arg='--self-test';Report='self-test.json';Timeout=60000},@{Arg='--ui-smoke';Report='ui-smoke.json';Timeout=30000},@{Arg='--notification-test';Report='notification-smoke.json';Timeout=10000})
    if($IncludeNativeInput){$taskChecks+=@{Arg='--control-test';Report='control-smoke.json';Timeout=12000}}
    foreach($taskCheck in $taskChecks){
        $taskSource=Join-Path $env:LOCALAPPDATA "OmniView/$($taskCheck.Report)"
        $taskStarted=[DateTime]::UtcNow
        $taskProcess=Start-Process -FilePath $taskExe -ArgumentList $taskCheck.Arg -WindowStyle Hidden -PassThru
        if(-not $taskProcess.WaitForExit($taskCheck.Timeout)){ $taskProcess.Kill();throw "$taskArch $($taskCheck.Arg): timeout" }
        if($taskProcess.ExitCode -ne 0){throw "$taskArch $($taskCheck.Arg): failed"}
        if(-not (Test-Path -LiteralPath $taskSource) -or (Get-Item -LiteralPath $taskSource).LastWriteTimeUtc -lt $taskStarted){throw 'Missing fresh report'}
        $taskReport=Get-Content -LiteralPath $taskSource -Raw | ConvertFrom-Json
        if($taskCheck.Arg -eq '--ui-smoke'){
            if(-not ($taskReport.font -and $taskReport.bridge -and $taskReport.search)){throw 'UI smoke failed'}
        }elseif(-not $taskReport.ok){throw ("Failed: "+($taskReport|ConvertTo-Json -Compress))}
        Copy-Item -LiteralPath $taskSource -Destination (Join-Path $taskOut $taskCheck.Report) -Force
        Write-Output "$taskArch $($taskCheck.Arg): passed"
    }
    Copy-Item -LiteralPath (Join-Path $env:LOCALAPPDATA 'OmniView/notification-preview.jpg') -Destination (Join-Path $taskOut 'notification-preview.jpg') -Force
}
