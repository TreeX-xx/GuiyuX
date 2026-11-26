param(
    [ValidateSet('Debug','Release')][string]$Profile = 'Release',
    [string]$OutputDirectory = (Join-Path $PSScriptRoot ('target/' + $Profile.ToLowerInvariant()))
)
$ErrorActionPreference = 'Stop'
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$stage = Join-Path $tempRoot ('GuiyuX-src-' + [guid]::NewGuid())
$cache = Join-Path $env:LOCALAPPDATA 'GuiyuX-build/cargo-target'
$settingsCache = Join-Path $PSScriptRoot 'target/settings-cargo'
$output = [IO.Path]::GetFullPath($OutputDirectory)
$web = Join-Path $stage 'settings'
$hostDir = Join-Path $web 'host'
New-Item -ItemType Directory -Path $hostDir -Force | Out-Null
$buildLock = [Threading.Mutex]::new($false, 'Local\GuiyuX.Build')
$ownsLock = $false
try {
    foreach ($item in 'Cargo.toml','Cargo.lock','assets','src') {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $item) -Destination $stage -Recurse
    }
    if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'build.rs')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build.rs') -Destination $stage
    }
    foreach ($item in 'package.json','package-lock.json','index.html','src') {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot "settings/$item") -Destination $web -Recurse
    }
    foreach ($item in 'Cargo.toml','Cargo.lock','build.rs','tauri.conf.json','src') {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot "settings/host/$item") -Destination $hostDir -Recurse
    }
    Push-Location $web
    try {
        & npm.cmd ci --no-audit --no-fund
        if ($LASTEXITCODE) { throw 'npm ci failed' }
        & npm.cmd run build
        if ($LASTEXITCODE) { throw 'Building the settings interface failed' }
    } finally { Pop-Location }
    # Both Cargo output names are shared by builds, including different checkouts.
    try { $ownsLock = $buildLock.WaitOne() }
    catch [Threading.AbandonedMutexException] { $ownsLock = $true }
    $flags = @('build','--locked')
    if ($Profile -eq 'Release') { $flags += '--release' }
    & cargo @flags --manifest-path "$stage/Cargo.toml" --target-dir $cache
    if ($LASTEXITCODE) { throw 'Building the resident core failed' }
    & cargo @flags --manifest-path "$hostDir/Cargo.toml" --target-dir $settingsCache
    if ($LASTEXITCODE) { throw 'Building the Tauri settings host failed' }

    New-Item -ItemType Directory -Force -Path $output | Out-Null
    $flavor = $Profile.ToLowerInvariant()
    $executables = @((Join-Path $cache "$flavor/guiyux.exe"), (Join-Path $settingsCache "$flavor/guiyux-settings.exe"))
    # Both executables have to come from the same build. Restore the previous pair on a copy failure.
    $installed = @()
    try {
        foreach ($source in $executables) {
            $name = [IO.Path]::GetFileName($source)
            $dest = Join-Path $output $name
            $backup = $null
            $pending = Join-Path $output ($name + '.' + [guid]::NewGuid() + '.tmp')
            try {
                [IO.File]::Copy($source, $pending)
                if ([IO.File]::Exists($dest)) {
                    $backup = Join-Path $output ($name + '.' + [guid]::NewGuid() + '.bak')
                    [IO.File]::Replace($pending, $dest, $backup)
                } else { [IO.File]::Move($pending, $dest) }
                $installed += [pscustomobject]@{ Path = $dest; Backup = $backup }
            } finally { [IO.File]::Delete($pending) }
        }
    } catch {
        foreach ($file in $installed) {
            if ($file.Backup) {
                [IO.File]::Replace($file.Backup, $file.Path, [System.Management.Automation.Language.NullString]::Value)
            }
            else { [IO.File]::Delete($file.Path) }
        }
        throw
    }
    foreach ($file in $installed) {
        if ($file.Backup) { [IO.File]::Delete($file.Backup) }
        Write-Output $file.Path
    }
} finally {
    if ($ownsLock) { $buildLock.ReleaseMutex() }
    $buildLock.Dispose()
    $resolved = (Resolve-Path -LiteralPath $stage).Path
    if ($resolved -eq $stage -and [IO.Path]::GetFileName($resolved) -like 'GuiyuX-src-*' -and
        [IO.Path]::GetDirectoryName($resolved) -eq $tempRoot.TrimEnd('\')) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
