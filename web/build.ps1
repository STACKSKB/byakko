param([string]$WasmBindgen = 'wasm-bindgen')
$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
Push-Location $repository
try {
    & cargo build --locked -p byakko-web --target wasm32-unknown-unknown --release
    if ($LASTEXITCODE -ne 0) { throw 'Browser Rust build failed' }
    & $WasmBindgen --target web --out-dir web/pkg --out-name byakko_web target/wasm32-unknown-unknown/release/byakko_web.wasm
    if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen generation failed (requires version 0.2.128)' }
} finally {
    Pop-Location
}
