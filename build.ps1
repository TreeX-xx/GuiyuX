param([ValidateSet('Debug','Release')][string]$Profile = 'Release')
$ErrorActionPreference = 'Stop'
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$stage = Join-Path $tempRoot ('GuiyuX-src-' + [guid]::NewGuid())
$cache = Join-Path $env:LOCALAPPDATA 'GuiyuX-build/cargo-target'
$output = Join-Path $PSScriptRoot ('target/' + $Profile.ToLowerInvariant())
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    foreach ($item in 'Cargo.toml','Cargo.lock','src') {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $item) -Destination $stage -Recurse
    }
    Push-Location $stage
    try {
        $flags = @('build','--locked','--target-dir',$cache)
        if ($Profile -eq 'Release') { $flags += '--release' }
        & cargo @flags
        if ($LASTEXITCODE -ne 0) { throw "Cargo build failed ($LASTEXITCODE)" }
        New-Item -ItemType Directory -Force -Path $output | Out-Null
        Copy-Item -LiteralPath (Join-Path $cache ($Profile.ToLowerInvariant() + '/guiyux.exe')) -Destination $output -Force
        Write-Output (Join-Path $output 'guiyux.exe')
    } finally { Pop-Location }
} finally {
    $resolved = (Resolve-Path -LiteralPath $stage).Path
    if ($resolved -eq $stage -and [IO.Path]::GetFileName($resolved) -like 'GuiyuX-src-*' -and
        [IO.Path]::GetDirectoryName($resolved) -eq $tempRoot.TrimEnd('\')) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
