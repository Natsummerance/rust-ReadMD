param([ValidateSet('aarch64-unknown-linux-ohos','x86_64-unknown-linux-ohos')][string]$Target='aarch64-unknown-linux-ohos')
$ErrorActionPreference='Stop'
$repo=Resolve-Path (Join-Path $PSScriptRoot '../../..')
if (!$env:OHOS_NDK_HOME) { throw 'Set OHOS_NDK_HOME to the existing DevEco native SDK. This script never downloads SDKs or dependencies.' }
$installed=(& rustup target list --installed)
if ($installed -notcontains $Target) { throw "Rust target $Target is not installed. Offline build cannot proceed." }
$arch=if ($Target.StartsWith('aarch64')) {'arm64-v8a'} else {'x86_64'}
$triple=if ($Target.StartsWith('aarch64')) {'aarch64-linux-ohos'} else {'x86_64-linux-ohos'}
$compiler=Join-Path $env:OHOS_NDK_HOME 'llvm/bin/clang.exe'
$compilerCpp=Join-Path $env:OHOS_NDK_HOME 'llvm/bin/clang++.exe'
$sysroot=Join-Path $env:OHOS_NDK_HOME 'sysroot'
if (!(Test-Path -LiteralPath $compiler) -or !(Test-Path -LiteralPath $sysroot)) { throw 'Native SDK Clang/sysroot unavailable. Check OHOS_NDK_HOME.' }
# The SDK does not supply Rust linker wrappers. Generate the documented OHOS
# target/sysroot/MUSL wrappers locally, without downloading a toolchain.
$wrappers=Join-Path $repo '.cache/harmony-toolchain'
New-Item -ItemType Directory -Force $wrappers | Out-Null
$clang=Join-Path $wrappers "$triple-clang.cmd"
$clangCpp=Join-Path $wrappers "$triple-clang++.cmd"
if (($compiler+$sysroot).IndexOfAny([char[]]'%&|<>"') -ge 0) { throw 'SDK path contains unsupported cmd metacharacters' }
Set-Content -LiteralPath $clang -Encoding ascii -Value "@echo off`r`n`"$compiler`" -target $triple --sysroot=`"$sysroot`" -D__MUSL__ %*`r`nexit /b %errorlevel%"
Set-Content -LiteralPath $clangCpp -Encoding ascii -Value "@echo off`r`n`"$compilerCpp`" -target $triple --sysroot=`"$sysroot`" -D__MUSL__ %*`r`nexit /b %errorlevel%"
$targetKey=$Target.ToUpper().Replace('-','_')
[Environment]::SetEnvironmentVariable("CARGO_TARGET_${targetKey}_LINKER",$clang,'Process')
[Environment]::SetEnvironmentVariable("CC_$($Target.Replace('-','_'))",$clang,'Process')
[Environment]::SetEnvironmentVariable("CXX_$($Target.Replace('-','_'))",$clangCpp,'Process')
[Environment]::SetEnvironmentVariable("AR_$($Target.Replace('-','_'))",(Join-Path $env:OHOS_NDK_HOME 'llvm/bin/llvm-ar.exe'),'Process')
if (!$env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR=Join-Path $repo '.cache/harmony-build' }
& cargo rustc --offline --locked --manifest-path (Join-Path $repo 'rust/Cargo.toml') -p readmd-kernel --no-default-features --features mobile-ffi --release --target $Target --lib -- --crate-type cdylib
if ($LASTEXITCODE) { throw 'Rust kernel cross build failed' }
$dest=Join-Path $PSScriptRoot "../entry/libs/$arch"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$Target/release/libreadmd_kernel.so") -Destination $dest
& node (Join-Path $PSScriptRoot 'sync-web-assets.mjs')
if ($LASTEXITCODE) { throw 'Offline assets sync failed' }
