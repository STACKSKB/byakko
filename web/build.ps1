param([string]$WasmBindgen = 'wasm-bindgen', [switch]$WordPress)
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
Push-Location $repository
try {
    & cargo build --locked -p byakko-web --target wasm32-unknown-unknown --release
    if ($LASTEXITCODE -ne 0) { throw 'Browser Rust build failed' }
    & $WasmBindgen --target web --out-dir web/pkg --out-name byakko_web target/wasm32-unknown-unknown/release/byakko_web.wasm
    if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen generation failed (requires version 0.2.128)' }
    if (-not (Test-Path web/node_modules/esbuild)) { throw 'Install browser build dependencies with npm ci --prefix web' }
    & npm --prefix web run build:static
    if ($LASTEXITCODE -ne 0) { throw 'Static browser packaging failed' }
    if ($WordPress) {
        & npm --prefix web run build:wordpress
        if ($LASTEXITCODE -ne 0) { throw 'WordPress packaging failed' }
        $pluginDirectory = Join-Path $repository 'web/dist/wordpress/byakko-configurator'
        $pluginArchive = Join-Path $repository 'web/dist/byakko-configurator.zip'
        Compress-Archive -LiteralPath $pluginDirectory -DestinationPath $pluginArchive -Force
        Write-Output "Built $pluginArchive"
    }
} finally {
    Pop-Location
}
