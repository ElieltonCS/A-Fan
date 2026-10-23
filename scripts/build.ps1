$ErrorActionPreference='Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    & cargo fmt --check;if($LASTEXITCODE){throw 'Formatação inválida.'}
    & cargo test --offline;if($LASTEXITCODE){throw 'Testes falharam.'}
    & cargo clippy --offline -- -D warnings;if($LASTEXITCODE){throw 'Clippy falhou.'}
    & cargo build --release --offline;if($LASTEXITCODE){throw 'Build falhou.'}
    $output=Join-Path (Get-Location) 'artifacts\release'
    New-Item -Path $output -ItemType Directory -Force|Out-Null
    Copy-Item -LiteralPath 'target\release\a-fan.exe' -Destination $output -Force
    foreach($name in @('install.ps1','set-startup.ps1','uninstall.ps1')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination $output -Force}
    Copy-Item -LiteralPath 'README.md' -Destination $output -Force
    $hash=Get-FileHash -LiteralPath (Join-Path $output 'a-fan.exe') -Algorithm SHA256
    [IO.File]::WriteAllText((Join-Path $output 'SHA256.txt'),($hash.Hash.ToLower()+'  a-fan.exe'+[Environment]::NewLine),(New-Object Text.UTF8Encoding($false)))
} finally {Pop-Location}
