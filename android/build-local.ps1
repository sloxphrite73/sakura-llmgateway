# build-local.ps1 - local Android APK build for sakura-llmgateway.
# Usage: pwsh android/build-local.ps1 [-Install]
#   Builds the universal APK (unsigned) entirely locally:
#   cargo-ndk cross-compile (3 ABIs) -> jniLibs -> gradle assembleRelease.
#   -Install also adb-installs to the single connected device/emulator.
# Toolchain lives in D:\android-toolchain (see PASS.md for the layout).
[CmdletBinding()] param([switch]$Install)

$ErrorActionPreference = 'Stop'
$root   = Split-Path $PSScriptRoot -Parent          # repo root
$tc     = 'D:\android-toolchain'
$sdk    = "$tc\sdk"
$env:JAVA_HOME = "$tc\jdk\jdk-17"
$env:ANDROID_HOME = $sdk
$env:ANDROID_SDK_ROOT = $sdk
$env:ANDROID_NDK_LATEST_HOME = "$sdk\ndk\26.3.11579264"
$env:ANDROID_NDK_HOME = $env:ANDROID_NDK_LATEST_HOME   # cargo-ndk asks for this one
$env:ANDROID_NDK = $env:ANDROID_NDK_LATEST_HOME
$env:PATH = "$tc\gradle\bin;$sdk\platform-tools;$sdk\cmdline-tools\latest\bin;$env:JAVA_HOME\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"

function Check($ok, $msg) { if (-not $ok) { Write-Host "MISSING: $msg" -ForegroundColor Red; exit 1 } }

Write-Host '== verify toolchain =='
Check (Test-Path "$env:JAVA_HOME\bin\java.exe") 'JDK 17 (expected D:\android-toolchain\jdk\jdk-17)'
Check (Test-Path "$sdk\cmdline-tools\latest\bin\sdkmanager.bat") 'cmdline-tools'
Check (Test-Path "$env:ANDROID_NDK_LATEST_HOME\ndk-build.cmd") 'NDK 26.3.11579264'
Check (Test-Path "$sdk\platforms\android-34\android.jar") 'platforms;android-34'
Check (Test-Path "$sdk\build-tools\34.0.0\aapt2.exe") 'build-tools;34.0.0'
Check (Test-Path "$sdk\platform-tools\adb.exe") 'platform-tools'
Check (Test-Path "$tc\gradle\bin\gradle.bat") 'gradle 8.9'

Write-Host '== accept sdk licenses =='
'y' * 8 | & "$sdk\cmdline-tools\latest\bin\sdkmanager.bat" --licenses | Out-Null

Write-Host '== cargo-ndk =='
if (-not (Get-Command cargo-ndk -ErrorAction SilentlyContinue)) { cargo install cargo-ndk --locked }

Write-Host '== cross-compile gateway (arm64-v8a, armeabi-v7a, x86_64) =='
Push-Location "$root\llm-gateway"
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -P 26 --manifest-path Cargo.toml -- build --release --locked
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
Pop-Location

Write-Host '== stage jniLibs =='
foreach ($abi in 'arm64-v8a','armeabi-v7a','x86_64') {
  $target = switch ($abi) { 'arm64-v8a' {'aarch64-linux-android'} 'armeabi-v7a' {'armv7-linux-androideabi'} 'x86_64' {'x86_64-linux-android'} }
  $dst = "$root\android\app\src\main\jniLibs\$abi"
  New-Item -ItemType Directory -Force $dst | Out-Null
  Copy-Item "$root\llm-gateway\target\$target\release\llm-gateway" "$dst\libllmgateway.so" -Force
}

Write-Host '== gradle assembleRelease =='
# Sign with the local dev keystore when present (a truly unsigned APK is
# uninstallable: "no certificates"). Dev key only — release signing stays in CI.
# NOTE: pass -P args literally, NOT via array splat — PS5.1 mangles splatted
# args containing '=' into one blob and gradle then reads a garbage keystore path.
$ks = "$tc\dev-keystore.jks"
Push-Location "$root\android"
if (Test-Path $ks) {
  Write-Host 'signing with local dev keystore'
  & "$tc\gradle\bin\gradle.bat" --no-daemon --console=plain assembleRelease `
    "-Pandroid.keystore.path=$ks" `
    "-Pandroid.keystore.password=sakura-dev" `
    "-Pandroid.keystore.alias=sakura-dev" `
    "-Pandroid.keystore.keypassword=sakura-dev"
} else {
  Write-Host 'WARNING: no dev keystore -> APK will be unsigned and uninstallable'
  & "$tc\gradle\bin\gradle.bat" --no-daemon --console=plain assembleRelease
}
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }
Pop-Location

$apkDir = "$root\android\app\build\outputs\apk\release"
$apk = Get-ChildItem $apkDir -Filter 'app-release*.apk' -ErrorAction SilentlyContinue | Sort-Object Length -Descending | Select-Object -First 1
Check ($null -ne $apk) 'APK output'
$out = "$root\sakura-llmgateway-local-universal.apk"
Copy-Item $apk.FullName $out -Force
Write-Host ("OK -> {0} ({1:N1} MB)" -f $out, ((Get-Item $out).Length/1MB))

if ($Install) {
  $devices = & adb devices | Select-String -Pattern '\tdevice$'
  if ($devices) { & adb install -r $out } else { Write-Host 'no adb device connected; skipping install' }
}
