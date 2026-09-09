# Produces a self-contained Windows x64 archive and the manifest embedded in Lumen.
param([string]$ReleaseTag = "cobalt-dev")
$ErrorActionPreference = 'Stop'
$commit = 'a636575b09de1fc55d9b8cd98cac88f5f2f16b42'
$nodeVersion = '22.23.2'
$root = (Get-Location).Path
$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$work = Join-Path $tempRoot ('cobalt-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
function Run($exe, $arguments) { & $exe @arguments; if ($LASTEXITCODE -ne 0) { throw "$exe failed" } }
Run git @('clone', 'https://github.com/imputnet/cobalt.git', "$work/source")
Push-Location "$work/source"
Run git @('checkout', '--detach', $commit)
Run pnpm @('--config.node-linker=hoisted', '--filter', '@imput/cobalt-api...', 'install', '--frozen-lockfile')
Run pnpm @('--config.node-linker=hoisted', '--filter', '@imput/cobalt-api', 'deploy', '--prod', "$work/bundle/api")
# Source archive and lockfile accompany every redistributable, without repository credentials.
Run git @('archive', '--format=zip', "--output=$work/cobalt-source.zip", $commit)
Pop-Location
# Upstream version-info reads .git at runtime. Ship immutable metadata, not a Git checkout.
$versionModule = @"
export const getVersion = async () => '11.7.1';
export const getCommit = async () => '$commit';
export const getBranch = async () => 'lumen-companion';
export const getRemote = async () => 'imputnet/cobalt';
"@
$versionModule | Set-Content "$work/bundle/api/node_modules/@imput/version-info/index.js" -Encoding utf8NoBOM
$versionModule | Set-Content "$work/bundle/version-info-replacement.js" -Encoding utf8NoBOM
Copy-Item "$work/cobalt-source.zip" "$work/bundle/"
Copy-Item "$work/source/api/LICENSE" "$work/bundle/COBALT-LICENSE"
Copy-Item "$work/source/pnpm-lock.yaml" "$work/bundle/"
$nodeZip = "node-v$nodeVersion-win-x64.zip"
Invoke-WebRequest "https://nodejs.org/dist/v$nodeVersion/$nodeZip" -OutFile "$work/$nodeZip"
Invoke-WebRequest "https://nodejs.org/dist/v$nodeVersion/SHASUMS256.txt" -OutFile "$work/SHASUMS256.txt"
$expected = ((Get-Content "$work/SHASUMS256.txt" | Where-Object { $_.EndsWith("  $nodeZip") }) -split ' ')[0]
if ((Get-FileHash "$work/$nodeZip" -Algorithm SHA256).Hash.ToLower() -ne $expected) { throw 'Node checksum mismatch' }
Expand-Archive "$work/$nodeZip" "$work/node"
Copy-Item "$work/node/node-v$nodeVersion-win-x64/node.exe" "$work/bundle/"
Copy-Item "$work/node/node-v$nodeVersion-win-x64/LICENSE" "$work/bundle/NODE-LICENSE"
@"
Cobalt 11.7.1, upstream commit $commit, API source with version-info metadata replacement (included as version-info-replacement.js).
https://github.com/imputnet/cobalt — AGPL-3.0. Source included in cobalt-source.zip.
Node $nodeVersion: https://nodejs.org/dist/v$nodeVersion/ (source and licenses).
Third-party notices and source references are in api/node_modules/*/package.json and LICENSE files.
FFmpeg is included by ffmpeg-static; its license/source reference is in that package.
"@ | Set-Content "$work/bundle/NOTICE.txt"
New-Item -ItemType Directory -Force "$root/dist" | Out-Null
$archive = "lumen-cobalt-11.7.1-node$nodeVersion-win-x64.zip"
Compress-Archive -Path "$work/bundle/*" -DestinationPath "$root/dist/$archive" -Force
$hash = (Get-FileHash "$root/dist/$archive" -Algorithm SHA256).Hash.ToLower()
@{ version = "11.7.1-$($hash.Substring(0,12))"; commit = $commit; sha256 = $hash; url = "https://github.com/Lumen-Connection/lumen-stream/releases/download/$ReleaseTag/$archive" } | ConvertTo-Json | Set-Content "$root/assets/cobalt/manifest.json" -Encoding utf8NoBOM
Copy-Item "$root/assets/cobalt/manifest.json" "$root/dist/cobalt-manifest.json"
# Verify the archive itself, never the build tree with its developer dependencies.
Expand-Archive "$root/dist/$archive" "$work/installed"
& "$work/installed/node.exe" --no-node-snapshot "$root/scripts/cobalt/smoke.mjs" "$work/installed"
if ($LASTEXITCODE -ne 0) { throw 'Companion smoke gate failed' }

if ($env:GITHUB_ENV) {
    "LUMEN_COBALT_TEST_BUNDLE=$work/installed" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
    "LUMEN_TEST_FFMPEG=$work/installed/api/node_modules/ffmpeg-static/ffmpeg.exe" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
}
