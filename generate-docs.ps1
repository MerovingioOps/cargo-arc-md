# generate-docs.ps1 - Automation script for generating SVG dependency diagrams
# and AI-optimized Markdown architecture documentation
#
# This script:
# 1. Checks if cargo-arc is installed, installs if not
# 2. Runs `cargo arc -o deps.svg` to generate SVG dependency diagrams
# 3. Generates ARCHITECTURE_DIAGRAMS.md with AI-optimized Mermaid diagrams
# 4. Generates README.md with workspace information
# 5. Generates individual crate markdown files in output-md/
#
# Usage: .\generate-docs.ps1 [workspace-path]
#   workspace-path: Path to the Cargo workspace (default: current directory)

param(
    [string]$WorkspacePath = "."
)

$ErrorActionPreference = "Stop"

# Colors for output
function Write-ColorOutput($ForegroundColor) {
    $fc = $host.UI.RawUI.ForegroundColor
    $host.UI.RawUI.ForegroundColor = $ForegroundColor
    if ($args) {
        Write-Output $args
    }
    $host.UI.RawUI.ForegroundColor = $fc
}

Write-ColorOutput Green "=== Architecture Documentation Generator ==="
Write-ColorOutput Yellow "Workspace path: $WorkspacePath"

# Get script directory
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$HelperBinary = Join-Path $ScriptDir "generate-docs-helper\target\release\cargo-arc-md.exe"

# Check if cargo-arc is installed
$CargoArcInstalled = $false
try {
    $null = cargo arc --version 2>&1
    $CargoArcInstalled = $true
    Write-ColorOutput Green "✓ cargo-arc found"
} catch {
    Write-ColorOutput Yellow "cargo-arc not found, installing..."
    cargo install cargo-arc
    Write-ColorOutput Green "✓ cargo-arc installed"
}

# Build the helper binary if needed
if (-not (Test-Path $HelperBinary)) {
    Write-ColorOutput Yellow "Building cargo-arc-md helper..."
    Push-Location (Join-Path $ScriptDir "generate-docs-helper")
    cargo build --release
    Pop-Location
    Write-ColorOutput Green "✓ Helper built"
}

# Run the helper to generate documentation
Write-ColorOutput Yellow "Generating documentation..."
& $HelperBinary $WorkspacePath

Write-ColorOutput Green "=== Documentation Generation Complete ==="
Write-Output ""
Write-Output "Generated files:"
Write-Output "  - $WorkspacePath\deps.svg"
Write-Output "  - $WorkspacePath\ARCHITECTURE_DIAGRAMS.md"
Write-Output "  - $WorkspacePath\README.md"
Write-Output "  - $WorkspacePath\output-md\ (individual crate documentation)"
Write-Output ""
