param([string]$RuntimeRoot, [string]$StorySeed, [string]$GameDirectory, [switch]$WaterFix, [switch]$DefaultBufferBudget, [switch]$CheckOnly,
    [switch]$Ztrx, [switch]$Combined, [switch]$TextureQueue, [switch]$TraceAccelerator, [switch]$PageFix, [switch]$PageFast, [switch]$BoatFix, [switch]$MaterialFix, [switch]$ManualPlay, [ValidateSet('Sync','Relay2','Late3','AsyncRelay2')][string]$Schedule='Sync')
$ErrorActionPreference='Stop'
if ($MaterialFix) { $BoatFix=$true }
if ($BoatFix) { $PageFast=$true; $env:VICTORY_TEXTURE_BACK='1' }
if ($PageFast) { $PageFix=$true }
if ($PageFix) { $TraceAccelerator=$true }
if ($TraceAccelerator) { $TextureQueue=$true; $ManualPlay=$true; $env:VICTORY_TRACE_ACCELERATOR="1" }
if ($TextureQueue) { $Combined=$true }
if ($Combined -and $Ztrx) { throw 'Choose one build: Combined or Ztrx.' }
if ($Schedule -eq 'AsyncRelay2' -and -not $Combined) { throw 'AsyncRelay2 requires -Combined.' }
if ($Ztrx -or $Combined) { $WaterFix=$true }
if ($Schedule -ne 'Sync' -and -not $ManualPlay) { throw 'Use -ManualPlay to compare scheduling modes.' }
if(-not $RuntimeRoot) { $RuntimeRoot=Join-Path $env:LOCALAPPDATA 'VIctory-fx-local\578d459' }
if (-not $StorySeed) {
    $candidates = foreach ($dir in (Get-ChildItem -LiteralPath (Join-Path $RuntimeRoot 'runs') -Directory)) {
        if ($dir.Name.StartsWith('combined-') -and -not (Test-Path -LiteralPath (Join-Path $dir.FullName 'pid.txt'))) { continue }
        $profile = Join-Path $dir.FullName 'data'
        $saves = Join-Path $profile '7952987A\saves'
        if (-not (Test-Path -LiteralPath $saves)) { continue }
        foreach ($blob in (Get-ChildItem -LiteralPath $saves -Filter '0' -File -Recurse)) {
            $slot = $blob.Directory.Parent.Name
            if ($slot -notmatch '^SARC\d+~' -or $blob.Length -le 0) { continue }
            if (-not (Test-Path -LiteralPath (Join-Path $blob.DirectoryName 'manifest')) -or
                -not (Test-Path -LiteralPath (Join-Path $blob.DirectoryName '1'))) { continue }
            [pscustomobject]@{Profile=$profile; Saved=$blob.LastWriteTimeUtc; RunStart=$dir.CreationTimeUtc}
        }
    }
    $chosen = $candidates | Sort-Object Saved,RunStart -Descending | Select-Object -First 1
    if (-not $chosen) { throw 'No previous story checkpoint found. Provide -StorySeed to select a profile.' }
    $StorySeed = $chosen.Profile
}
if(-not $GameDirectory) { $GameDirectory=Join-Path (Split-Path (Split-Path $PSScriptRoot -Parent) -Parent) 'GTA6.Xbox.v1.0.2.1-CYBERLEEK' }
if(-not (Test-Path -LiteralPath (Join-Path $GameDirectory 'game_scarlett_master.exe'))) { throw "Game executable missing at $GameDirectory" }
if(-not (Test-Path -LiteralPath (Join-Path $StorySeed '7952987A'))) { throw "Story seed missing at $StorySeed" }
$binary=if ($MaterialFix) { 'victory-material-plane.exe' } elseif ($BoatFix) { 'victory-boat-water.exe' } elseif ($PageFast) { 'victory-page-fast.exe' } elseif ($PageFix) { 'victory-page-fix.exe' } elseif ($TraceAccelerator) { 'victory-input-trace.exe' } elseif ($TextureQueue) { 'victory-scene-texture-queue.exe' } elseif ($Combined) { 'victory-scene-combined.exe' } elseif ($Ztrx) { 'victory-scene-ztrx.exe' } elseif ($WaterFix) { 'victory-scene-water.exe' } else { 'victory-scene-fixed.exe' }
$exe=Join-Path $PSScriptRoot $binary
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Missing executable: $exe" }
if (-not $ManualPlay -and -not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'geometry-packs.index') -PathType Leaf)) { throw 'Missing geometry index' }
if ($WaterFix) {
    if ($DefaultBufferBudget) {
        $env:VICTORY_CARD_BUFFERS_MIN_MB='16'
        $env:VICTORY_CARD_BUFFERS_MB='512'
    } else {
        $env:VICTORY_CARD_BUFFERS_MIN_MB='1'
        $env:VICTORY_CARD_BUFFERS_MB='1024'
    }
}
if ($ManualPlay) {
    foreach ($name in @('KEYS','CAPTURE','CAPTURE_FRAMES','CAPTURE_WORLD','CAPTURE_WORLD_ALSO','DUMP_TARGETS','ROOM_FRAMES','ROOM_TRIGGER','ROOM_WORLD_AFTER','MODEL_SAMPLES','GEOMETRY_INDEX','SCENE_ASSET_IO','LATE_WAIT','WIDE_BACK','RELAY','GPU_TIMING','ASYNC_SUBMIT','QUEUE2','CARD_FETCH_INFLIGHT','INDIRECT_WINDOW','PROFILE','SKIP_PIPELINES','SKIP_PIPELINES_FILE')) {
        Remove-Item -LiteralPath ('Env:\VICTORY_' + $name) -ErrorAction SilentlyContinue
    }
    if ($Schedule -eq 'AsyncRelay2') { $env:VICTORY_ASYNC_SUBMIT='1'; $env:VICTORY_QUEUE2='1' }
    if ($Schedule -in @('Relay2','AsyncRelay2')) { $env:VICTORY_RELAY='2' }
    if ($Schedule -eq 'Late3') { $env:VICTORY_LATE_WAIT='3'; $env:VICTORY_WIDE_BACK='1' }
}
if ($Combined) { $env:VICTORY_FORCE_MUTE='1' }
if ($CheckOnly) {
    [pscustomobject]@{Executable=$exe;Seed=$StorySeed;Cache=(Join-Path $RuntimeRoot 'cache');ForceMute=$env:VICTORY_FORCE_MUTE;ManualPlay=[bool]$ManualPlay;Schedule=$Schedule;AsyncSubmit=$env:VICTORY_ASYNC_SUBMIT;CopyQueue=$env:VICTORY_QUEUE2;Relay=$env:VICTORY_RELAY;LateWait=$env:VICTORY_LATE_WAIT;WideBack=$env:VICTORY_WIDE_BACK;BufferThresholdMiB=$env:VICTORY_CARD_BUFFERS_MIN_MB;BufferBudgetMiB=$env:VICTORY_CARD_BUFFERS_MB}
    return
}
$knownNames=@('victory-room-trace','victory-fx-local','victory-geometry-trace','victory-model-samples','victory-scene-fixed','victory-scene-water','victory-scene-ztrx','victory-scene-combined','victory-scene-texture-queue','victory-input-trace','victory-page-fix','victory-page-fast','victory-boat-water','victory-material-plane')
$editions=(Split-Path -Parent $PSScriptRoot) + [IO.Path]::DirectorySeparatorChar
foreach($proc in @(Get-Process -Name $knownNames -ErrorAction SilentlyContinue)) {
    if ($proc.Path -and $proc.Path.StartsWith($editions, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Game is already running (PID $($proc.Id))."
    }
}
$tag=if ($MaterialFix) { "combined-materialplane-$Schedule-" } elseif ($BoatFix) { "combined-boatwater-$Schedule-" } elseif ($PageFast) { "combined-pagefast-$Schedule-" } elseif ($PageFix) { "combined-pagefix-$Schedule-" } elseif ($TraceAccelerator) { "combined-input-$Schedule-" } elseif ($TextureQueue) { "combined-queue-$Schedule-" } elseif ($Combined) { "combined-f85101b-$Schedule-" } elseif ($Ztrx) { "ztrx93-$Schedule-" } elseif ($ManualPlay) { "manual-$Schedule-" } elseif ($WaterFix) { 'water-4e17478-' } else { 'fixed-4e17478-' }
$run=Join-Path $RuntimeRoot ('runs\'+$tag+(Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $run -ErrorAction Stop | Out-Null
if (-not $ManualPlay) {
    $env:VICTORY_CAPTURE=Join-Path $run 'frames'
    New-Item -ItemType Directory -Path $env:VICTORY_CAPTURE | Out-Null
}
$env:VICTORY_DATA_DIR=Join-Path $run 'data'
Copy-Item -LiteralPath $StorySeed -Destination $env:VICTORY_DATA_DIR -Recurse
$savesPath=Join-Path $env:VICTORY_DATA_DIR '7952987A\saves'
function Get-CheckpointHash([string]$Path) {
    $stream=[IO.File]::OpenRead($Path)
    $algorithm=[Security.Cryptography.SHA256]::Create()
    try { [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','') }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}
$proof=foreach ($b in (Get-ChildItem -LiteralPath $savesPath -Filter '0' -File -Recurse)) {
    if($b.Directory.Parent.Name -notmatch '^SARC\d+~'){ continue }
    Push-Location -LiteralPath $env:VICTORY_DATA_DIR
    try { $relative=Resolve-Path -LiteralPath $b.FullName -Relative } finally { Pop-Location }
    $original=Join-Path $StorySeed $relative
    $hash=(Get-CheckpointHash $b.FullName)
    if($hash -ne (Get-CheckpointHash $original)){throw "Story copy differs: $relative"}
    [pscustomobject]@{Slot=$b.Directory.Parent.Name;Generation=$b.Directory.Name;Bytes=$b.Length;SavedUtc=$b.LastWriteTimeUtc.ToString('o');SHA256=$hash}
}
[pscustomobject]@{Seed=$StorySeed;Checkpoints=@($proof)} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $run 'story-check.json') -Encoding utf8
$env:VICTORY_GAME_DIR=$GameDirectory
$env:VICTORY_CACHE_DIR=Join-Path $RuntimeRoot 'cache'
$env:VICTORY_DISPLAY_MODE='1920x1080'
if (-not $ManualPlay) {
    Remove-Item -LiteralPath 'Env:\VICTORY_ROOM_FRAMES' -ErrorAction SilentlyContinue
    $env:VICTORY_ROOM_WORLD_AFTER='2,6,16,30,80,160,240,320,400'
    $env:VICTORY_ROOM_TRIGGER='1'
    $env:VICTORY_MODEL_SAMPLES='1'
    $env:VICTORY_GEOMETRY_INDEX=Join-Path $PSScriptRoot 'geometry-packs.index'
    if(-not (Test-Path -LiteralPath $env:VICTORY_GEOMETRY_INDEX)){ throw 'Missing geometry-packs.index' }
    $env:VICTORY_CAPTURE_WORLD='1'
    $env:VICTORY_SCENE_ASSET_IO=Join-Path $run 'archive-reads.tsv'
    $env:VICTORY_KEYS='12:Enter,16:Enter,20:Enter,24:Enter,28:Enter,32:Enter,36:Enter'
    foreach ($name in @('LATE_WAIT','RELAY','SKIP_PIPELINES','SKIP_PIPELINES_FILE','DUMP_TARGETS','CAPTURE_FRAMES','CAPTURE_WORLD_ALSO')) {
        Remove-Item -LiteralPath ('Env:\VICTORY_' + $name) -ErrorAction SilentlyContinue
    }
}
$env:RUST_LOG='info'
$env:VICTORY_LOG='info'
$game=Join-Path $GameDirectory 'game_scarlett_master.exe'
$proc=Start-Process -FilePath $exe -ArgumentList @('run',('"{0}"' -f $game),'--console','series-x','--execute') -WorkingDirectory $PSScriptRoot -RedirectStandardError (Join-Path $run 'game.log') -RedirectStandardOutput (Join-Path $run 'out.log') -PassThru
Set-Content -LiteralPath (Join-Path $run 'pid.txt') -Value $proc.Id
$marker=if ($Combined) { 'last-combined-run.txt' } elseif ($Ztrx) { 'last-ztrx93-run.txt' } elseif ($WaterFix) { 'last-water-4e17478-run.txt' } else { 'last-fixed-4e17478-run.txt' }
Set-Content -LiteralPath (Join-Path $RuntimeRoot $marker) -Value $run
Write-Output ("PID {0} | {1}" -f $proc.Id,$run)

& (Join-Path $PSScriptRoot 'set-game-window.ps1') -GameProcessId $proc.Id
