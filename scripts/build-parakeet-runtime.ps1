param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot "..\bin")
)

$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true
$repository = "https://github.com/mudler/parakeet.cpp.git"
$commit = "e8acc6172a94e20a952cf1843decace5d771a94b"
$work = Join-Path ([System.IO.Path]::GetTempPath()) "zui-parakeet-$commit"
$patch = Join-Path $PSScriptRoot "..\patches\parakeet-server-language.patch"

if (Test-Path -LiteralPath $work) {
    Remove-Item -LiteralPath $work -Recurse -Force
}

git clone --recursive $repository $work
git -C $work checkout $commit
git -C $work submodule update --init --recursive
git -C $work apply --check $patch
git -C $work apply $patch

$build = Join-Path $work "build"
cmake -S $work -B $build -A x64 `
    "-DPARAKEET_VERSION=0.4.0-zui.1" `
    "-DPARAKEET_BUILD_CLI=OFF" `
    "-DPARAKEET_BUILD_SERVER=ON" `
    "-DPARAKEET_BUILD_TESTS=OFF" `
    "-DBUILD_SHARED_LIBS=OFF" `
    "-DGGML_BACKEND_DL=OFF" `
    "-DGGML_NATIVE=OFF"
cmake --build $build --config Release --target parakeet-server

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$binary = Join-Path $build "examples\server\Release\parakeet-server.exe"
$destination = Join-Path $OutputDirectory "parakeet-server.exe"
Copy-Item -LiteralPath $binary -Destination $destination -Force

$file = Get-Item -LiteralPath $destination
$hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "Built $destination"
Write-Output "Size: $($file.Length)"
Write-Output "SHA-256: $hash"
