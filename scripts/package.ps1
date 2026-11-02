param([ValidateSet('x64','x86')][string]$Architecture = 'x64', [switch]$SkipBuild, [string]$RustTarget = '')
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
Push-Location $taskRoot
try {
    $taskCargo = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe'
    $taskTarget = if ($Architecture -eq 'x86') { 'i686-pc-windows-gnullvm' } else { '' }
    if ($RustTarget) { $taskTarget=$RustTarget }
    $taskLoaderArch = if ($Architecture -eq 'x86') { 'x86' } else { 'x64' }
    $taskExpectedMachine = if ($Architecture -eq 'x86') { 0x14c } else { 0x8664 }
    if (-not $SkipBuild) {
        $taskBuildArgs = @('build','--release','--locked')
        if ($taskTarget) { $taskBuildArgs += @('--target',$taskTarget) }
        & $taskCargo @taskBuildArgs
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    }
    $taskDist = Join-Path $taskRoot $(if ($Architecture -eq 'x86') { 'dist/win32' } else { 'dist' })
    $taskBinary = if ($taskTarget) { "target/$taskTarget/release/omniview.exe" } else { 'target/release/omniview.exe' }
    New-Item -ItemType Directory -Path $taskDist -Force | Out-Null
    $taskOld = Join-Path $taskDist 'OmniView.exe'
    $taskBackup = Join-Path $taskRoot 'legacy-native/OmniView.exe'
    if ($Architecture -eq 'x64' -and (Test-Path -LiteralPath $taskOld) -and -not (Test-Path -LiteralPath $taskBackup)) {
        Copy-Item -LiteralPath $taskOld -Destination $taskBackup
    }
    Copy-Item -LiteralPath $taskBinary -Destination (Join-Path $taskDist 'OmniView.exe')
    Copy-Item -LiteralPath $taskBinary -Destination (Join-Path $taskDist 'OmniView-Student.exe')
    # GNU builds dynamically link the Microsoft WebView2 loader.
    $taskMetadata = & $taskCargo metadata --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $taskVersion = ($taskMetadata.packages | Where-Object name -eq 'omniview' | Select-Object -First 1).version
    $taskWebview = $taskMetadata.packages | Where-Object name -eq 'webview2-com-sys' | Select-Object -First 1
    if (-not $taskWebview) { throw 'WebView2 loader package not found' }
    $taskLoader = Join-Path (Split-Path $taskWebview.manifest_path -Parent) "$taskLoaderArch/WebView2Loader.dll"
    Copy-Item -LiteralPath $taskLoader -Destination (Join-Path $taskDist 'WebView2Loader.dll')
    foreach ($taskPe in @('OmniView.exe','OmniView-Student.exe','WebView2Loader.dll')) {
        $taskBytes = [IO.File]::ReadAllBytes((Join-Path $taskDist $taskPe))
        $taskPeOffset = [BitConverter]::ToInt32($taskBytes,0x3c)
        $taskMachine = [BitConverter]::ToUInt16($taskBytes,$taskPeOffset+4)
        if ($taskMachine -ne $taskExpectedMachine) { throw "Wrong architecture: $taskPe" }
    }
    Copy-Item -LiteralPath 'README.md' -Destination (Join-Path $taskDist 'README.md')
    Copy-Item -LiteralPath 'assets/fonts/OFL.txt' -Destination (Join-Path $taskDist 'Google-Sans-OFL.txt')
    $taskFonts = Join-Path $taskDist 'presentation-fonts'
    New-Item -ItemType Directory -Path $taskFonts -Force | Out-Null
    foreach ($taskFont in @('GoogleSans-Regular.ttf','GoogleSans-Bold.ttf','OFL.txt')) {
        Copy-Item -LiteralPath (Join-Path $taskRoot "assets/fonts/$taskFont") -Destination $taskFonts
    }
    $taskFiles = @('OmniView.exe','OmniView-Student.exe','WebView2Loader.dll','README.md','Google-Sans-OFL.txt')
    $taskHashes = foreach ($taskName in $taskFiles) {
        $taskFile = Join-Path $taskDist $taskName
        [ordered]@{name=$taskName; architecture=$Architecture; version=$taskVersion; bytes=(Get-Item -LiteralPath $taskFile).Length; sha256=(Get-FileHash -LiteralPath $taskFile -Algorithm SHA256).Hash.ToLowerInvariant()}
    }
    $taskHashes | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskDist 'manifest.json') -Encoding utf8
    $taskZipPaths = @($taskFiles | ForEach-Object {Join-Path $taskDist $_}) + @($taskFonts,(Join-Path $taskDist 'manifest.json'))
    $taskSuffix = if ($Architecture -eq 'x86') { 'win32' } else { 'win64' }
    Compress-Archive -LiteralPath $taskZipPaths -DestinationPath (Join-Path $taskRoot "dist/OmniView-$taskVersion-$taskSuffix.zip") -Force
    Get-ChildItem -LiteralPath $taskDist | Select-Object Name,Length
} finally { Pop-Location }
