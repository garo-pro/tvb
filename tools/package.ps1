# Packages a release build into dist/TV-Blind.zip plus a SHA-256 file.
# Expects `cargo build --release` and THIRD-PARTY-LICENSES.html to exist.
# The zip's file names matter: the updater looks for TV-Blind.zip and
# extracts it over the app folder, so TV-Blind.exe must sit at the top level.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$stage = "dist/TV-Blind"
Remove-Item dist -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target/release/tvb.exe "$stage/TV-Blind.exe"
Copy-Item README.md, LICENSE, THIRD-PARTY-LICENSES.html, docs/privacy-policy.md, docs/eula.md $stage
Compress-Archive -Path "$stage/*" -DestinationPath dist/TV-Blind.zip
$hash = (Get-FileHash dist/TV-Blind.zip -Algorithm SHA256).Hash.ToLower()
"$hash  TV-Blind.zip" | Out-File dist/TV-Blind.zip.sha256 -Encoding ascii -NoNewline
Write-Output "packaged dist/TV-Blind.zip ($hash)"
