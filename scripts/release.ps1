# Fail-closed release script for Windows PowerShell and PowerShell 7.

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
    $PSNativeCommandUseErrorActionPreference = $true
}

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Description,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command
    )

    & $Command
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "$Description failed with exit code $exitCode"
    }
}

function Invoke-NativeCapture {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Description,
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command
    )

    $output = & $Command
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "$Description failed with exit code $exitCode"
    }
    return (($output | ForEach-Object { $_.ToString() }) -join "`n").Trim()
}

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
Push-Location -LiteralPath $repoRoot
try {
    $cargoToml = Get-Content -LiteralPath "Cargo.toml" -Raw
    $versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"\s*$')
    if (-not $versionMatch.Success) {
        throw "Could not determine the package version from Cargo.toml"
    }
    $version = $versionMatch.Groups[1].Value
    $tag = "v$version"

    Write-Host "=== Neo Decompiler $tag Release ===" -ForegroundColor Cyan

    $branch = Invoke-NativeCapture "determine current branch" { git branch --show-current }
    if ($branch -ne "master") {
        throw "Releases must run from master; current branch is '$branch'"
    }

    $worktree = Invoke-NativeCapture "inspect working tree" { git status --porcelain }
    if ($worktree.Length -ne 0) {
        throw "Release requires a clean working tree"
    }

    Invoke-Native "fetch origin/master and tags" { git fetch --quiet origin master --tags }
    $head = Invoke-NativeCapture "resolve HEAD" { git rev-parse HEAD }
    $originMaster = Invoke-NativeCapture "resolve origin/master" { git rev-parse origin/master }
    if ($head -ne $originMaster) {
        throw "HEAD must exactly match origin/master before release"
    }

    $existingTag = Invoke-NativeCapture "check whether tag $tag exists" { git tag --list $tag }
    if ($existingTag -eq $tag) {
        throw "Tag $tag already exists"
    }

    Write-Host "Step 1: Checking formatting..." -ForegroundColor Yellow
    Invoke-Native "cargo fmt" { cargo fmt --all -- --check }

    Write-Host "Step 2: Running clippy..." -ForegroundColor Yellow
    Invoke-Native "cargo clippy" { cargo clippy --locked --all-targets --all-features -- -D warnings }
    Invoke-Native "cargo clippy --no-default-features" { cargo clippy --locked --all-targets --no-default-features -- -D warnings }

    Write-Host "Step 3: Running tests..." -ForegroundColor Yellow
    Invoke-Native "cargo test" { cargo test --locked --all-features }
    Invoke-Native "cargo test --no-default-features" { cargo test --locked --no-default-features }

    Write-Host "Step 4: Running benchmarks..." -ForegroundColor Yellow
    Invoke-Native "cargo bench" { cargo bench --locked }

    Write-Host "Step 5: Building release..." -ForegroundColor Yellow
    Invoke-Native "cargo build --release" { cargo build --locked --release }

    Write-Host "Step 6: Generating docs..." -ForegroundColor Yellow
    $previousRustdocFlags = $env:RUSTDOCFLAGS
    try {
        $env:RUSTDOCFLAGS = "$previousRustdocFlags -D warnings".Trim()
        Invoke-Native "cargo doc" { cargo doc --locked --no-deps }
    } finally {
        $env:RUSTDOCFLAGS = $previousRustdocFlags
    }

    Write-Host "Step 7: Checking the JavaScript packages..." -ForegroundColor Yellow
    Invoke-Native "standalone JavaScript package tests" { npm --prefix js test }
    Invoke-Native "npm ci" { npm --prefix web ci --ignore-scripts }
    Invoke-Native "web package version check" { npm --prefix web run version:check }
    Invoke-Native "web package tests" { npm --prefix web test }
    Invoke-Native "build and verify web archive" { npm --prefix web run verify:pack }
    Invoke-Native "real WebAssembly report APIs" { npm --prefix web run test:wasm }

    Write-Host "Step 8: Checking the crates.io package..." -ForegroundColor Yellow
    Invoke-Native "cargo package" { cargo package --locked }
    Invoke-Native "cargo publish --dry-run" { cargo publish --locked --dry-run }

    Write-Host "=== All checks passed for $tag. ===" -ForegroundColor Green
    $response = Read-Host "Publish neo-decompiler $version to crates.io? (y/n)"
    if ($response -eq "y" -or $response -eq "Y") {
        $currentHead = Invoke-NativeCapture "recheck HEAD" { git rev-parse HEAD }
        $currentWorktree = Invoke-NativeCapture "recheck working tree" { git status --porcelain }
        if ($currentHead -ne $head -or $currentWorktree.Length -ne 0) {
            throw "Checkout changed during release checks; rerun them before publishing"
        }
        Invoke-Native "cargo publish" { cargo publish --locked }
        Invoke-Native "create signed tag $tag" { git tag -s $tag $head -m "neo-decompiler $tag" }
        Invoke-Native "push tag $tag" { git push origin $tag }
        Write-Host "=== Published and pushed $tag successfully. ===" -ForegroundColor Green
    } else {
        Write-Host "Skipped publishing. After review, run:" -ForegroundColor Yellow
        Write-Host "  cargo publish --locked"
        Write-Host "  git tag -s $tag -m 'neo-decompiler $tag'"
        Write-Host "  git push origin $tag"
    }
} finally {
    Pop-Location
}
