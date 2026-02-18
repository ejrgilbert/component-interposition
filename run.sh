#!/usr/bin/env bash
# =============================================================================
# run.sh - Workflow runner for service + middleware + composition
#
# Features:
#   - Robust environment check with version validation and missing tool warnings
#   - Auto-install hints for missing tools (Linux/macOS)
#   - Building components (service/middlewares)
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
# Some helpful globals
# -----------------------------------------------------------------------------
PATH_WASI_TARGET="./target/wasm32-wasip1/debug"
PATH_COMPOSED="./compositions"
PATH_DECOMP="./decomposer"
PATH_WAC="./wac"

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
    echo -e "${BLUE}Usage: $0 [command] [option]${NC}"
    echo ""
    echo -e "${BLUE}Commands:${NC}"
    echo -e "  env         : Check the environment and verify required tools and versions"
    echo -e "  build       : Build the service and middleware components"
    echo -e "  compose     : Compose the service and middleware(s) into a single component"
    echo -e "  run         : Run the composed component"
    echo -e "  run-service : Run the service standalone (without middleware(s))"
    echo -e "  all         : Run the full workflow: env check, build service, build middlewares, compose, and run"
    echo -e "  __testme    : A utility to help quickly test this script (exercises all run configs)"
    echo -e "  --help|-h   : Show this usage message"
    echo ""
    echo -e "${BLUE}Options:${NC}"
    echo -e "  --single    : Wrap the service call with a SINGLE middleware (a)"
    echo -e "  --multiple  : Wrap the service call with a MULTIPLE middlewares (a, b, and c)"
    echo -e "  --chained-services : Perform service chaining on the services (a and b)"
    echo -e "  --splice1   : Splice a component with two services directly communicating with a SINGLE middleware (a)"
    echo -e "  --spliceAll : Splice a component with two services directly communicating with MULTIPLE middlewares (a, b, and c)"
    echo ""
}

# -----------------------------------------------------------------------------
# Helper: Auto-install hints
# -----------------------------------------------------------------------------
CARGO_INST="cargo"
install_hint() {
    local tool=$1
    local fmt=$2

    echo -e "${YELLOW}Hint: You can install $tool with:${NC}"
    if [[ "$fmt" == "$CARGO_INST" ]]; then
        echo "  cargo install $tool"
    else
        echo "  macOS: brew install $tool"
        echo "  Linux: sudo apt update && sudo apt install -y $tool"
    fi
}

# -----------------------------------------------------------------------------
# Environment check
# -----------------------------------------------------------------------------
check_env() {
    log_info "Checking environment configuration..."
    MISSING_TOOLS=0

    REQUIRED_TOOLS=(
        "cargo:1.93.0:brew"
        "wasm-tools:1.244.0:$CARGO_INST"
        "wkg:0.13.0:$CARGO_INST"
        "splicer:1.0.0:$CARGO_INST"
        "wac:0.9.0:$CARGO_INST"
    )

    for tool_version in "${REQUIRED_TOOLS[@]}"; do
        tool="$(echo "$tool_version" | cut -d ':' -f 1)"
        expected="$(echo "$tool_version" | cut -d ':' -f 2)"
        fmt="$(echo "$tool_version" | cut -d ':' -f 3)"

        if ! command -v "$tool" &> /dev/null; then
            log_error "$tool is not installed or not in PATH!"
            install_hint "$tool" "$fmt"
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

check_encoding() {
    local fmt="$1"
    local wasm_file="$2"
    local wat_file="$3"
    local name
    name=$(basename "$wasm_file")
    log_info "Checking that $name $fmt WAT is valid..."
    if ! wasm-tools print "$wasm_file" -o "$wat_file"; then
        log_error "Failed to generate WAT for $output_wasm"
        exit 1
    fi

    if ! head -n 1 "$wat_file" | grep -q "($fmt"; then
        log_error "$name WAT check failed: expected a $fmt."
        exit 1
    fi
    log_success "$name WAT is a valid $fmt"
}

# -----------------------------------------------------------------------------
# Generic component builder
# Arguments:
#   1 = component name (for logging, e.g., "service_a")
#   2 = directory (e.g., "service_a")
#   3 = wasm base name (e.g., "service_a")
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
    popd > /dev/null

    local PTH_MOD="$PATH_WASI_TARGET/${base}.wasm"
    local PTH_MOD_WAT="$PATH_WASI_TARGET/${base}.wat"
    local PTH_COMP="$PATH_WASI_TARGET/${base}.comp.wasm"
    local PTH_COMP_WAT="$PATH_WASI_TARGET/${base}.comp.wat"
    local ADAPTER_PTH="./wasi_snapshot_preview1.reactor.wasm"

    check_encoding "module" "$PTH_MOD" "$PTH_MOD_WAT"

    log_info "Converting '$name' to a component..."
    if ! wasm-tools component new "$PTH_MOD" --adapt "$ADAPTER_PTH" --skip-validation -o "$PTH_COMP"; then
        log_error "Generating a component from the compiled module failed for $name!"
        exit 1
    fi
    check_encoding "component" "$PTH_COMP" "$PTH_COMP_WAT"

    log_success "'$name' component built successfully!"
}

# -----------------------------------------------------------------------------
# Compose service and middleware
# -----------------------------------------------------------------------------
compose() {
    log_info "Composing service and middleware(s)..."

    case "$1" in
        --single)
            compose_single
            ;;
        --multiple)
            compose_multiple
            ;;
        --chained-services)
            compose_chained_services
            ;;
        --splice1)
            compose --chained-services
            compose_splice1
            ;;
        --spliceAll)
            compose --chained-services
            compose_spliceAll
            ;;
        *)
            log_error "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac
}

# -----------------------------------------------------------------------------
# Generic wrapper for invoking `wac compose`.
# -----------------------------------------------------------------------------
run_wac() {
    local wac_file="$1"
    local output_wasm="$2"
    shift 2

    log_info "Running wac compose using $(basename "$wac_file")..."
    if ! wac compose "$wac_file" "$@" --output "$output_wasm"; then
        log_error "Composition with '$(basename "$wac_file")' completed failed!"
        exit 1
    fi
    check_encoding "component" "$output_wasm" "$(basename "$output_wasm").wat"

    log_success "Composition with '$(basename "$wac_file")' completed successfully!"
}
compose_single() {
    run_wac \
        "$PATH_WAC/composition-single.wac" \
        "$PATH_COMPOSED/composed-single.wasm" \
          --dep my:service="$PATH_WASI_TARGET/service_b.comp.wasm" \
          --dep my:middleware="$PATH_WASI_TARGET/middleware_a.comp.wasm"
}
compose_multiple() {
    run_wac \
        "$PATH_WAC/composition-multiple.wac" \
        "$PATH_COMPOSED/composed-multiple.wasm" \
          --dep my:service="$PATH_WASI_TARGET/service_b.comp.wasm" \
          --dep my:middleware-a="$PATH_WASI_TARGET/middleware_a.comp.wasm" \
          --dep my:middleware-b="$PATH_WASI_TARGET/middleware_b.comp.wasm" \
          --dep my:middleware-c="$PATH_WASI_TARGET/middleware_c.comp.wasm"
}
compose_chained_services() {
    run_wac \
        "$PATH_WAC/composition-service_chaining.wac" \
        "$PATH_COMPOSED/service-chaining.wasm" \
          --dep my:service-a="$PATH_WASI_TARGET/service_a.comp.wasm" \
          --dep my:service-b="$PATH_WASI_TARGET/service_b.comp.wasm"
}
compose_splice1() {
    log_info "Splitting the chained component..."
    pushd $PATH_DECOMP > /dev/null

    if ! cargo run -- "../$PATH_COMPOSED/service-chaining.wasm"; then
        log_error "Failed to split out the chained component."
        exit 1
    fi

    popd > /dev/null
    log_success "Successfully split the chained component."

    run_wac \
        "$PATH_WAC/composition-splice1.wac" \
        "$PATH_COMPOSED/spliced1.wasm" \
          --dep my:service-a="$PATH_DECOMP/split1.wasm" \
          --dep my:service-b="$PATH_DECOMP/split0.wasm" \
          --dep my:middleware="$PATH_WASI_TARGET/middleware_a.comp.wasm"
}
compose_spliceAll() {
    # TODO
    log_error "We do not support component splicing of multiple middlewares yet."
    exit 1
}

# -----------------------------------------------------------------------------
# Run the composed component
# -----------------------------------------------------------------------------
run() {
    local component=$1

    if [[ ! -f "$component" ]]; then
        log_error "Component not found at '$component'! Please run the 'build' and 'compose' steps first."
        exit 1
    fi

    log_info "Running component at $component..."
    pushd runner > /dev/null

    if ! cargo run -- "../$component"; then
        log_error "Failed to run the component at $component."
        exit 1
    fi

    popd > /dev/null
    log_success "Component at $component ran successfully!"
}
run_services() {
    PATH_SVC_B="$PATH_WASI_TARGET/service_b.comp.wasm"
    CHAINED="$PATH_COMPOSED/service-chaining.wasm"

    run $PATH_SVC_B
    run $CHAINED
}
run_composition() {
    case "$1" in
        --single)
            COMPOSED="$PATH_COMPOSED/composed-single.wasm"
            ;;
        --multiple)
            COMPOSED="$PATH_COMPOSED/composed-multiple.wasm"
            ;;
        --chained-services)
            COMPOSED="$PATH_COMPOSED/service-chaining.wasm"
            ;;
        --splice1)
            COMPOSED="$PATH_COMPOSED/spliced1.wasm"
            ;;
        --spliceAll)
            log_error "We do not support component splicing of multiple middlewares yet."
            exit 1
            ;;
        *)
            log_error "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac

    run "$COMPOSED"
}

build() {
    check_env
    build_component "service_a" "service_a" "service_a"
    build_component "service_b" "service_b" "service_b"
    build_component "middleware_a" "middleware_a" "middleware_a"
    build_component "middleware_b" "middleware_b" "middleware_b"
    build_component "middleware_c" "middleware_c" "middleware_c"
    compose --chained-services
}

run_tests() {
    implemented_options=("--single" "--multiple" "--chained-services" "--splice1")
    log_info "Running all different configurations, these should all execute successfully!\n"

    check_env
    build

    # Iterate and build a command for each item
    for opt in "${implemented_options[@]}"; do
        log_info "Executing with option: $opt"
        compose "$opt"
        run_composition "$opt"
        log_info "Option completed successfully: $opt"
    done

    echo
    log_success "All configurations of this run.sh script still work! BOOYAH! :)"
}

# -----------------------------------------------------------------------------
# Parse command line argument
# -----------------------------------------------------------------------------
CMD="${1:-all}"
OPT="${2:---single}"

case "$CMD" in
    env)
        check_env
        ;;
    build)
        build
        ;;
    compose)
        check_env
        compose "$OPT"
        ;;
    run)
        check_env
        run_composition "$OPT"
        ;;
    run-service)
        check_env
        run_services
        ;;
    all)
        build
        compose "$OPT"
        run_composition "$OPT"
        log_success "All steps completed successfully!"
        ;;
    __testme)
        run_tests
        ;;
    --help|-h)
        print_usage
        ;;
    *)
        log_error "Unknown option: $CMD"
        print_usage
        exit 1
        ;;
esac
