#!/usr/bin/env bash
# =============================================================================
# run.sh - Workflow runner for service + middleware + composition
#
# Features:
#   - Robust environment check with version validation and missing tool warnings
#   - Auto-install hints for missing tools (Linux/macOS)
#   - Building components (service/middleware)
#   - Composing the service and middleware with wac
#   - Running the composed component
#   - Full workflow mode
#   - Colored logs: info (blue), warning (yellow), error (red), success (green)
#
# Usage:
#   ./run.sh [env|service|middleware|compose|run|all]
# =============================================================================

set -euo pipefail

# -----------------------------------------------------------------------------
# Color codes for logs
# -----------------------------------------------------------------------------
BLUE="\033[1;34m"
YELLOW="\033[1;33m"
RED="\033[1;31m"
GREEN="\033[1;32m"
NC="\033[0m"  # No Color

# -----------------------------------------------------------------------------
# Logging functions
# -----------------------------------------------------------------------------
log_info()    { echo -e "${BLUE}[INFO] $1${NC}"; }
log_warning() { echo -e "${YELLOW}[WARNING] $1${NC}"; }
log_error()   { echo -e "${RED}[ERROR] $1${NC}"; }
log_success() { echo -e "${GREEN}[SUCCESS] $1${NC}"; }

# -----------------------------------------------------------------------------
# Print usage with descriptions
# -----------------------------------------------------------------------------
print_usage() {
    echo -e "${BLUE}Usage: $0 [option]${NC}"
    echo ""
    echo -e "${BLUE}Options:${NC}"
    echo -e "  env         : Check the environment and verify required tools and versions"
    echo -e "  service     : Build the service component"
    echo -e "  middleware  : Build the middleware component"
    echo -e "  compose     : Compose the service and middleware into a single component"
    echo -e "  run         : Run the composed component"
    echo -e "  all         : Run the full workflow: env check, build service, build middleware, compose, and run"
    echo -e "  --help      : Show this usage message"
    echo ""
}

# -----------------------------------------------------------------------------
# Helper: Auto-install hints
# -----------------------------------------------------------------------------
install_hint() {
    local tool=$1
    echo -e "${YELLOW}Hint: You can install $tool with:${NC}"
    echo "  macOS: brew install $tool"
    echo "  Linux: sudo apt update && sudo apt install -y $tool"
}

# -----------------------------------------------------------------------------
# Environment check
# -----------------------------------------------------------------------------
check_env() {
    log_info "Checking environment configuration..."
    MISSING_TOOLS=0

    REQUIRED_TOOLS=(
        "cargo:1.93.0"
        "wasm-tools:1.244.0"
        "wkg:0.13.0"
        "wac:0.9.0-dev"
    )

    for tool_version in "${REQUIRED_TOOLS[@]}"; do
        tool="${tool_version%%:*}"
        expected="${tool_version##*:}"

        if ! command -v "$tool" &> /dev/null; then
            log_error "$tool is not installed or not in PATH!"
            install_hint "$tool"
            MISSING_TOOLS=1
            continue
        fi

        actual=$($tool --version 2>/dev/null || echo "unknown")

        if [[ "$actual" != *"$expected"* ]]; then
            log_warning "$tool version mismatch. Expected: $expected, Found: $actual"
        else
            log_info "$tool version OK: $actual"
        fi
    done

    if [[ $MISSING_TOOLS -eq 1 ]]; then
        log_error "One or more required tools are missing. Please install them before continuing."
        exit 1
    fi

    log_success "Environment check passed!"
}

# -----------------------------------------------------------------------------
# Generic component builder
# Arguments:
#   1 = component name (for logging, e.g., "service")
#   2 = directory (e.g., "service")
#   3 = wasm base name (e.g., "service")
# -----------------------------------------------------------------------------
build_component() {
    local name=$1
    local dir=$2
    local base=$3

    log_info "Building '$name' component..."
    pushd "$dir" > /dev/null

    log_info "Fetching wit dependencies..."
    if ! wkg wit fetch; then
        log_error "wkg wit fetch failed for $name. Exiting."
        exit 1
    fi

    log_info "Compiling $name to wasm32-wasip1..."
    if ! cargo build --target wasm32-wasip1; then
        log_error "Cargo build failed for $name. Exiting."
        exit 1
    fi

    local PTH_MOD="./target/wasm32-wasip1/debug/${base}.wasm"
    local PTH_MOD_WAT="./target/wasm32-wasip1/debug/${base}.wat"
    local PTH_COMP="./target/wasm32-wasip1/debug/${base}.comp.wasm"
    local PTH_COMP_WAT="./target/wasm32-wasip1/debug/${base}.comp.wat"
    local ADAPTER_PTH="../wasi_snapshot_preview1.reactor.wasm"

    # ------------------------------
    # Programmatic check: is MODULE
    # ------------------------------
    log_info "Checking that $name module WAT is valid..."
    wasm-tools print "$PTH_MOD" -o "$PTH_MOD_WAT"

    if ! head -n 1 "$PTH_MOD_WAT" | grep -q "(module"; then
        log_error "$name WAT check failed: expected a MODULE."
        exit 1
    fi
    log_success "$name WAT is a valid MODULE"

    log_info "Converting '$name' to a component..."
    if ! wasm-tools component new "$PTH_MOD" --adapt "$ADAPTER_PTH" --skip-validation -o "$PTH_COMP"; then
        log_error "Generating a component from the compiled module failed for $name!"
        exit 1
    fi

    # ------------------------------
    # Programmatic check: is COMPONENT
    # ------------------------------
    log_info "Checking that $name component WAT is valid..."
    wasm-tools print "$PTH_COMP" -o "$PTH_COMP_WAT"

    if ! head -n 1 "$PTH_COMP_WAT" | grep -q "(component"; then
        log_error "$name component WAT check failed: expected a COMPONENT."
        exit 1
    fi
    log_success "$name WAT is a valid COMPONENT"

    popd > /dev/null
    log_success "'$name' component built successfully!"
}

# -----------------------------------------------------------------------------
# Compose service and middleware
# -----------------------------------------------------------------------------
compose() {
    log_info "Composing service and middleware..."

    PATH_SVC="./service/target/wasm32-wasip1/debug/service.comp.wasm"
    PATH_MDL="./middleware/target/wasm32-wasip1/debug/middleware.comp.wasm"
    OUTPUT="composed.wasm"
    OUTPUT_WAT="composed.wat"

    if ! wac compose composition.wac \
          --dep my:service="$PATH_SVC" \
          --dep my:middleware="$PATH_MDL" \
          --output "$OUTPUT"; then
        log_error "Creating the composition of the service+middleware failed"
        exit 1
    fi

    log_info "Checking WAT output of composed component..."
    ls -al "$OUTPUT"
    wasm-tools print "$OUTPUT" -o "$OUTPUT_WAT"

    log_success "Composition completed successfully!"
}

# -----------------------------------------------------------------------------
# Run the composed component
# -----------------------------------------------------------------------------
run_composition() {
    COMPOSED="./composed.wasm"

    if [[ ! -f "$COMPOSED" ]]; then
        log_error "Composed component not found! Please run the compose step first."
        exit 1
    fi

    log_info "Running composed component..."
    pushd runner > /dev/null

    cargo run -- "../$COMPOSED"

    popd > /dev/null
    log_success "Composition ran successfully!"
}

# -----------------------------------------------------------------------------
# Parse command line argument
# -----------------------------------------------------------------------------
ARG="${1:-all}"

case "$ARG" in
    env)
        check_env
        ;;
    service)
        check_env
        build_component "service" "service" "service"
        ;;
    middleware)
        check_env
        build_component "middleware" "middleware" "middleware"
        ;;
    compose)
        check_env
        compose
        ;;
    run)
        check_env
        run_composition
        ;;
    all)
        check_env
        build_component "service" "service" "service"
        build_component "middleware" "middleware" "middleware"
        compose
        run_composition
        log_success "All steps completed successfully!"
        ;;
    --help|-h)
        print_usage
        ;;
    *)
        log_error "Unknown option: $ARG"
        print_usage
        exit 1
        ;;
esac
