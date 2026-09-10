$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "`n========================================================" -ForegroundColor Cyan
    Write-Host " [GATE] $Message" -ForegroundColor Cyan
    Write-Host "========================================================" -ForegroundColor Cyan
}

function Check-Result {
    param([string]$StepName)
    if ($LASTEXITCODE -ne 0) {
        Write-Host "`n[FAILED] $StepName failed with exit code $LASTEXITCODE" -ForegroundColor Red
        exit $LASTEXITCODE
    }
    Write-Host "[PASSED] $StepName" -ForegroundColor Green
}

$startTime = Get-Date

# Constrain concurrent build jobs to prevent Windows paging file exhaustion (os error 1455)
$env:CARGO_BUILD_JOBS = "2"

Write-Host "Starting OpenVideoAI Phase 2 Quality Gates..." -ForegroundColor Magenta

# 1. Frontend Typecheck
Write-Step "1/6: Frontend TypeScript Check (npm run typecheck)"
npm run typecheck
Check-Result "Frontend TypeScript Check"

# 2. Frontend Unit & Component Tests
Write-Step "2/6: Frontend Tests (npm run test)"
npm run test
Check-Result "Frontend Tests"

# 3. Frontend Production Build
Write-Step "3/6: Frontend Build (npm run build)"
npm run build
Check-Result "Frontend Build"

# 4. Backend Rust Formatting Check
Write-Step "4/6: Rust Formatting (cargo fmt --check)"
cargo fmt --manifest-path src-tauri/Cargo.toml --check
Check-Result "Rust Formatting Check"

# 5. Backend Rust Clippy Lints (Zero Warnings)
Write-Step "5/6: Rust Clippy Lints (cargo clippy --all-targets -- -D warnings)"
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
Check-Result "Rust Clippy Check"

# 6. Backend Rust Unit & Integration Tests
Write-Step "6/6: Backend Tests (cargo test)"
cargo test --manifest-path src-tauri/Cargo.toml
Check-Result "Backend Tests"

$elapsed = (Get-Date) - $startTime
Write-Host "`n========================================================" -ForegroundColor Green
Write-Host " ALL PHASE 2 QUALITY GATES PASSED in $($elapsed.TotalSeconds.ToString("F1"))s" -ForegroundColor Green
Write-Host "========================================================`n" -ForegroundColor Green
exit 0
