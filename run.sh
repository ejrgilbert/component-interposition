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

# Resolve repo root from script location so `wkg wit fetch` finds the registry
# config regardless of which component subdir we pushd into.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Anchor cwd so relative paths emitted by external tools (splicer, wac) resolve
# correctly regardless of where the script was invoked from. Safe in an
# executed script: only affects this subshell, not the caller's shell.
cd "$REPO_ROOT"
export WKG_CONFIG_FILE="$REPO_ROOT/wkg-registries.toml"

# -----------------------------------------------------------------------------
# Some helpful globals
# -----------------------------------------------------------------------------
PATH_WASI_TARGET="./target/wasm32-wasip1/debug"
PATH_COMPOSED="./compositions"
PATH_WAC="./generated-wac"
PATH_RULES="./splicer-rules"
PATH_FIXTURES="./fixtures"
PATH_HANDLERS="./handlers"
PATH_PROXY_MDL="./middleware"
PATH_FAN_IN="./fan-in"

mkdir -p $PATH_COMPOSED $PATH_WAC $PATH_FIXTURES

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
    echo -e "  --single        : Wrap the service call with a SINGLE middleware (a)"
    echo -e "  --multiple      : Wrap the service call with a MULTIPLE middlewares (a, b, and c)"
    echo -e "  --chain         : Perform service chaining on the services (a and b)"
    echo -e "  --chain1        : Splice a component with two services directly communicating with a SINGLE middleware (a)"
    echo -e "  --chainN        : Splice a component with N services directly communicating with MULTIPLE middlewares (a, b, and c)"
    echo -e "  --nested        : Perform service chaining and create a chain where one of the nodes contains a chain"
    echo -e "  --inner-nested1 : Splice the nested chain node in a chained composition with a SINGLE middleware (a)"
    echo -e "  --inner-nestedN : Splice the nested chain node in a chained composition with MULTIPLE middlewares (a, b, and c)"
    echo -e "  --pre-nested1   : Splice the nested composition before the nested node in the chain with a SINGLE middleware (a)"
    echo -e "  --pre-nestedN   : Splice the nested composition before the nested node in the chain with MULTIPLE middlewares (a, b, and c)"
    echo -e "  --inner+pre-nested1 : Splice the nested composition BOTH before AND inside nested chain node with a SINGLE middleware (a)"
    echo -e "  --fanin         : Perform service chaining and create a chain where multiple downstream dependencies are chained to a single service"
    echo -e "  --fanin1        : Splice fan-in topology composition with a SINGLE middleware on a specific downstream dependency call"
    echo -e "  --faninN        : Splice fan-in topology composition with MULTIPLE middlewares on a specific downstream dependency call"
    echo -e "  --fanin-all1    : Splice fan-in topology composition with a SINGLE middleware between all downstream dependency calls"
    echo -e "  --fanin-allN    : Splice fan-in topology composition with MULTIPLE middlewares between all downstream dependency calls"
    echo -e "  --block1        : Splice middleware that chooses to block the downstream call"
    echo -e "  --blockN        : Splice multiple middlewares where the middle-most one chooses to block any further calls (the last middleware and the original dependency)"
    echo -e "  --noblock1      : Splice middleware that chooses to NOT block the downstream call"
    echo -e "  --noblockN      : Splice multiple middlewares where the middle-most one chooses to NOT block any further calls (the last middleware and the original dependency)"
    echo -e "  --tier2         : Splice the tier-2 typed-logger middleware on the fan-in adder interface (logs lifted arg values via on-call)"
    echo -e "  --skip-build    : Skip the build step (use with \`all\` when fixtures/ already holds built components, e.g. in parallel test harnesses)"
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
        "wasm-tools:1.247.0:$CARGO_INST"
        "wkg:0.13.0:$CARGO_INST"
        "splicer:2.3.0:$CARGO_INST"
        "cviz-cli:2.0.3:$CARGO_INST"
        "wac:0.10.0:$CARGO_INST"
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

    if [[ -f wkg.lock ]]; then
        log_info "Fetching wit dependencies..."
        if ! wkg wit fetch; then
            log_error "wkg wit fetch failed for $name. Exiting."
            exit 1
        fi
    else
        log_info "No wkg.lock — assuming WIT deps are vendored under wit/deps/."
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
        --chain)
            compose_chain
            ;;
        --chain1)
            compose --chain
            compose_chain1
            ;;
        --chainN)
            compose --chain
            compose_chainN
            ;;
        --nested)
            compose_nested
            ;;
        --inner-nested1)
            compose --nested
            compose_inner_nested1
            ;;
        --inner-nestedN)
            compose --nested
            compose_inner_nestedN
            ;;
        --pre-nested1)
            compose --nested
            compose_pre_nested1
            ;;
        --pre-nestedN)
            compose --nested
            compose_pre_nestedN
            ;;
        --inner+pre-nested1)
            compose --nested
            compose_inner_pre_nested1
            ;;
        --inner+pre-nestedN)
            compose --nested
            compose_inner_pre_nestedN
            ;;
        --fanin)
            compose_fanin
            ;;
        --fanin1)
            compose_fanin1
            ;;
        --faninN)
            compose_faninN
            ;;
        --fanin-all1)
            compose_fanin_all1
            ;;
        --fanin-allN)
            compose_fanin_allN
            ;;
        --block1)
            compose_block1
            ;;
        --blockN)
            compose_blockN
            ;;
        --nonblock1)
            compose_block1
            ;;
        --nonblockN)
            compose_blockN
            ;;
        --tier2)
            compose_tier2
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

run_splicer() {
    local wasm_file="$1"
    local rule_file="$2"
    local output_wac="$3"
    local output_wasm="$4"
    shift 4

    log_info "Running splicer with rule set '$(basename "$rule_file")'..."
    if ! wac_cmd=$(splicer splice "$rule_file" "$wasm_file" -o "$output_wac") ; then
        log_error "Splice with '$(basename "$rule_file")' failed! Used the following command:"
        echo splicer splice "$rule_file" "$wasm_file" -o "$output_wac"
        exit 1
    fi
    log_success "Splicer generated splits and a wac composition with '$(basename "$rule_file")' successfully!"

    log_info "Running the wac composition generated for '$(basename "$rule_file")'..."
    if ! eval "$wac_cmd  -o $output_wasm"; then
        log_error "Failed to run wac command:"
        echo "$wac_cmd"
        exit 1
    fi
    log_success "The 'wac compose' ran successfully '$(basename "$rule_file")'..."
}
run_splicer_solver() {
      local output_wac="$1"
      local output_wasm="$2"
      shift 2
      local -a wasm_files=("$@")

    log_info "Running splicer composition solver..."
    if ! wac_cmd=$(splicer compose "${wasm_files[@]}" -o "$output_wac") ; then
        log_error "Splice composition solver failed! Used the following command:"
        echo splicer compose "${wasm_files[@]}" -o "$output_wac"
        exit 1
    fi
    log_success "Splicer successfully solved the composition!"

    log_info "Running the wac composition generated..."
    if ! eval "$wac_cmd  -o $output_wasm"; then
        log_error "Failed to run wac command:"
        echo "$wac_cmd"
        exit 1
    fi
    log_success "The 'wac compose' ran successfully..."
}

compose_single() {
    run_splicer \
        "$PATH_FIXTURES/service_b.comp.wasm" \
        "$PATH_RULES/single.yaml" \
        "$PATH_WAC/single.wac" \
        "$PATH_COMPOSED/single.wasm"
}
compose_multiple() {
    run_splicer \
        "$PATH_FIXTURES/service_b.comp.wasm" \
        "$PATH_RULES/multiple.yaml" \
        "$PATH_WAC/multiple.wac" \
        "$PATH_COMPOSED/multiple.wasm"
}
compose_chain() {
    run_splicer \
        "$PATH_FIXTURES/service_b.comp.wasm" \
        "$PATH_RULES/chain.yaml" \
        "$PATH_WAC/chain.wac" \
        "$PATH_COMPOSED/chained.wasm"
}
compose_chain1() {
    local wasm_file="$PATH_FIXTURES/chained.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/chain1.yaml" \
        "$PATH_WAC/chain1.wac" \
        "$PATH_COMPOSED/chain1.wasm"
}
compose_chainN() {
    local wasm_file="$PATH_FIXTURES/chained.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/chainN.yaml" \
        "$PATH_WAC/chainN.wac" \
        "$PATH_COMPOSED/chainN.wasm"
}
compose_nested() {
    run_splicer \
        "$PATH_FIXTURES/chained.wasm" \
        "$PATH_RULES/nested.yaml" \
        "$PATH_WAC/nested.wac" \
        "$PATH_COMPOSED/nested.wasm"
}
compose_inner_nested1() {
    local wasm_file="$PATH_FIXTURES/nested.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/inner-nested1.yaml" \
        "$PATH_WAC/inner-nested1.wac" \
        "$PATH_COMPOSED/inner-nested1.wasm"
}
compose_inner_nestedN() {
    local wasm_file="$PATH_FIXTURES/nested.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/inner-nestedN.yaml" \
        "$PATH_WAC/inner-nestedN.wac" \
        "$PATH_COMPOSED/inner-nestedN.wasm"
}
compose_pre_nested1() {
    local wasm_file="$PATH_FIXTURES/nested.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/pre-nested1.yaml" \
        "$PATH_WAC/pre-nested1.wac" \
        "$PATH_COMPOSED/pre-nested1.wasm"
}
compose_pre_nestedN() {
    local wasm_file="$PATH_FIXTURES/nested.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/pre-nestedN.yaml" \
        "$PATH_WAC/pre-nestedN.wac" \
        "$PATH_COMPOSED/pre-nestedN.wasm"
}
compose_inner_pre_nested1() {
    local wasm_file="$PATH_FIXTURES/nested.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/inner-pre-nested1.yaml" \
        "$PATH_WAC/inner-pre-nested1.wac" \
        "$PATH_COMPOSED/inner-pre-nested1.wasm"
}
compose_inner_pre_nestedN() {
    local wasm_file="$PATH_FIXTURES/nested.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/inner-pre-nestedN.yaml" \
        "$PATH_WAC/inner-pre-nestedN.wac" \
        "$PATH_COMPOSED/inner-pre-nestedN.wasm"
}
compose_fanin() {
    run_splicer_solver \
        "$PATH_WAC/fanin.wac" \
        "$PATH_COMPOSED/fanin.wasm" \
        "$PATH_FIXTURES/service.comp.wasm" \
        "$PATH_FIXTURES/adder.comp.wasm" \
        "$PATH_FIXTURES/adder_async.comp.wasm" \
        "$PATH_FIXTURES/messenger.comp.wasm" \
        "$PATH_FIXTURES/messenger_async.comp.wasm" \
        "$PATH_FIXTURES/printer1.comp.wasm" \
        "$PATH_FIXTURES/printer1_async.comp.wasm" \
        "$PATH_FIXTURES/printer_n.comp.wasm" \
        "$PATH_FIXTURES/shapes.comp.wasm" \
        "$PATH_FIXTURES/shapes_handles.comp.wasm"
}
compose_fanin1() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/fanin1.yaml" \
        "$PATH_WAC/fanin1.wac" \
        "$PATH_COMPOSED/fanin1.wasm"
}
compose_faninN() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/faninN.yaml" \
        "$PATH_WAC/faninN.wac" \
        "$PATH_COMPOSED/faninN.wasm"
}
compose_fanin_all1() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/fanin-all1.yaml" \
        "$PATH_WAC/fanin-all1.wac" \
        "$PATH_COMPOSED/fanin-all1.wasm"
}
compose_fanin_allN() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/fanin-allN.yaml" \
        "$PATH_WAC/fanin-allN.wac" \
        "$PATH_COMPOSED/fanin-allN.wasm"
}
compose_block1() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/block1.yaml" \
        "$PATH_WAC/block1.wac" \
        "$PATH_COMPOSED/block1.wasm"
}
compose_blockN() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/blockN.yaml" \
        "$PATH_WAC/blockN.wac" \
        "$PATH_COMPOSED/blockN.wasm"
}
compose_tier2() {
    local wasm_file="$PATH_FIXTURES/fanin.wasm"
    run_splicer \
        "$wasm_file" \
        "$PATH_RULES/tier2.yaml" \
        "$PATH_WAC/tier2.wac" \
        "$PATH_COMPOSED/tier2.wasm"
}

# -----------------------------------------------------------------------------
# Run the composed component
# -----------------------------------------------------------------------------
run() {
    local component=$1
    local env_vars=$2
    local expected_file=$3  # optional: path to expected output file

    if [[ ! -f "$component" ]]; then
        log_error "Component not found at '$component'! Please run the 'build' and 'compose' steps first."
        exit 1
    fi

    log_info "Running component at $component..."
    pushd runner > /dev/null

    local output
    if ! output=$(eval "$env_vars cargo run -- \"../$component\"" 2>&1); then
        log_error "Failed to run the component at $component."
        echo "$output"
        exit 1
    fi
    echo "$output"

    popd > /dev/null

    # Validate output against expected file if provided
    if [[ -n "$expected_file" && -f "$expected_file" ]]; then
        # Filter runner output the same way expected files were captured:
        # strip ANSI codes, blank lines, cargo build noise
        local filtered
        filtered=$(echo "$output" \
            | sed 's/\x1b\[[0-9;]*m//g' \
            | grep -v "Compiling\|Finished\|Running.*target/debug/runner\|Downloading\|Downloaded\|Updating\|Locking\|Adding\|available:\|cache" \
            | sed '/^[[:space:]]*$/d' \
        )
        local expected
        expected=$(sed '/^[[:space:]]*$/d' "$expected_file")
        if [[ "$filtered" != "$expected" ]]; then
            log_error "Output mismatch for $component!"
            echo "--- Expected (from $expected_file) ---"
            echo "$expected"
            echo "--- Actual ---"
            echo "$filtered"
            echo "--- Diff ---"
            diff <(echo "$expected") <(echo "$filtered") || true
            exit 1
        fi
        log_success "Output matches expected for $(basename "$expected_file" .txt)"
    fi

    log_success "Component at $component ran successfully!"
}
run_services() {
    PATH_SVC_A="$PATH_FIXTURES/service_a.comp.wasm"
    PATH_SVC_B="$PATH_FIXTURES/service_b.comp.wasm"
    PATH_SVC_C="$PATH_FIXTURES/service_c.comp.wasm"
    CHAINED="$PATH_COMPOSED/chained.wasm"
    NESTED="$PATH_COMPOSED/nested.wasm"
    INNER_NESTED1="$PATH_COMPOSED/inner-nested1.wasm"
    INNER_NESTED_N="$PATH_COMPOSED/inner-nestedN.wasm"
    PRE_NESTED1="$PATH_COMPOSED/pre-nested1.wasm"
    PRE_NESTED_N="$PATH_COMPOSED/pre-nestedN.wasm"
    INNER_PRE_NESTED1="$PATH_COMPOSED/inner-pre-nested1.wasm"
    INNER_PRE_NESTED_N="$PATH_COMPOSED/inner-pre-nestedN.wasm"
    FANIN="$PATH_COMPOSED/fanin.wasm"
    FANIN1="$PATH_COMPOSED/fanin1.wasm"
    FANIN_N="$PATH_COMPOSED/faninN.wasm"
    FANIN_ALL1="$PATH_COMPOSED/fanin-all1.wasm"
    FANIN_ALL_N="$PATH_COMPOSED/fanin-allN.wasm"
    BLOCK1="$PATH_COMPOSED/block1.wasm"
    BLOCK_N="$PATH_COMPOSED/blockN.wasm"

    run $PATH_SVC_A
    run $PATH_SVC_B
    run $PATH_SVC_C
    run $CHAINED
    run $NESTED
    run $INNER_NESTED1
    run $INNER_NESTED_N
    run $PRE_NESTED1
    run $PRE_NESTED_N
    run $INNER_PRE_NESTED1
    run $INNER_PRE_NESTED_N
    run $FANIN
    run $FANIN1
    run $FANIN_N
    run $FANIN_ALL1
    run $FANIN_ALL_N

    run $BLOCK1 "SHOULD_BLOCK=true"
    run $BLOCK_N "SHOULD_BLOCK=true"

    run $BLOCK1 "SHOULD_BLOCK=false"
    run $BLOCK_N "SHOULD_BLOCK=false"
}
run_composition() {
    ENV_VARS=""
    case "$1" in
        --single)
            COMPOSED="$PATH_COMPOSED/single.wasm"
            ;;
        --multiple)
            COMPOSED="$PATH_COMPOSED/multiple.wasm"
            ;;
        --chain)
            COMPOSED="$PATH_FIXTURES/chained.wasm"
            ;;
        --chain1)
            COMPOSED="$PATH_COMPOSED/chain1.wasm"
            ;;
        --chainN)
            COMPOSED="$PATH_COMPOSED/chainN.wasm"
            ;;
        --nested)
            COMPOSED="$PATH_FIXTURES/nested.wasm"
            ;;
        --inner-nested1)
            COMPOSED="$PATH_COMPOSED/inner-nested1.wasm"
            ;;
        --inner-nestedN)
            COMPOSED="$PATH_COMPOSED/inner-nestedN.wasm"
            ;;
        --pre-nested1)
            COMPOSED="$PATH_COMPOSED/pre-nested1.wasm"
            ;;
        --pre-nestedN)
            COMPOSED="$PATH_COMPOSED/pre-nestedN.wasm"
            ;;
        --inner+pre-nested1)
            COMPOSED="$PATH_COMPOSED/inner-pre-nested1.wasm"
            ;;
        --inner+pre-nestedN)
            COMPOSED="$PATH_COMPOSED/inner-pre-nestedN.wasm"
            ;;
        --fanin)
            COMPOSED="$PATH_FIXTURES/fanin.wasm"
            ;;
        --fanin1)
            COMPOSED="$PATH_COMPOSED/fanin1.wasm"
            ;;
        --faninN)
            COMPOSED="$PATH_COMPOSED/faninN.wasm"
            ;;
        --fanin-all1)
            COMPOSED="$PATH_COMPOSED/fanin-all1.wasm"
            ;;
        --fanin-allN)
            COMPOSED="$PATH_COMPOSED/fanin-allN.wasm"
            ;;
        --block1)
            COMPOSED="$PATH_COMPOSED/block1.wasm"
            ENV_VARS="SHOULD_BLOCK=true"
            ;;
        --blockN)
            COMPOSED="$PATH_COMPOSED/blockN.wasm"
            ENV_VARS="SHOULD_BLOCK=true"
            ;;
        --nonblock1)
            COMPOSED="$PATH_COMPOSED/block1.wasm"
            ENV_VARS="SHOULD_BLOCK=false"
            ;;
        --nonblockN)
            COMPOSED="$PATH_COMPOSED/blockN.wasm"
            ENV_VARS="SHOULD_BLOCK=false"
            ;;
        --tier2)
            COMPOSED="$PATH_COMPOSED/tier2.wasm"
            ;;
        *)
            log_error "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac

    # Derive the expected output file name from the option (strip leading --)
    local opt_name="${1#--}"
    local expected_file="./expected-output/${opt_name}.txt"

    log_info "Visualization of the component at $COMPOSED:"
    viz "$COMPOSED" "$1"
    echo

    run "$COMPOSED" "$ENV_VARS" "$expected_file"
}

# -----------------------------------------------------------------------------
# Visualize the component
# -----------------------------------------------------------------------------
viz() {
    local component=$1
    local opt=${2:-}
    # Fan-in topologies aren't a `handler` chain (services connect via
    # `my:service/*` interfaces), so the default --detail handler-chain mode
    # reports "No service chains found". Switch to --detail full for fan-in
    # and its descendants so the connections are visible.
    case "$opt" in
        --fanin*|--block*|--nonblock*|--tier2)
            cviz-cli --detail full "$component"
            ;;
        *)
            cviz-cli "$component"
            ;;
    esac
}
viz_composition() {
    case "$1" in
        --single)
            COMPOSED="$PATH_COMPOSED/single.wasm"
            ;;
        --multiple)
            COMPOSED="$PATH_COMPOSED/multiple.wasm"
            ;;
        --chain)
            COMPOSED="$PATH_FIXTURES/chained.wasm"
            ;;
        --chain1)
            COMPOSED="$PATH_COMPOSED/chain1.wasm"
            ;;
        --chainN)
            COMPOSED="$PATH_COMPOSED/chainN.wasm"
            ;;
        --nested)
            COMPOSED="$PATH_FIXTURES/nested.wasm"
            ;;
        --inner-nested1)
            COMPOSED="$PATH_COMPOSED/inner-nested1.wasm"
            ;;
        --inner-nestedN)
            COMPOSED="$PATH_COMPOSED/inner-nestedN.wasm"
            ;;
        --pre-nested1)
            COMPOSED="$PATH_COMPOSED/pre-nested1.wasm"
            ;;
        --pre-nestedN)
            COMPOSED="$PATH_COMPOSED/pre-nestedN.wasm"
            ;;
        --inner+pre-nested1)
            COMPOSED="$PATH_COMPOSED/inner-pre-nested1.wasm"
            ;;
        --inner+pre-nestedN)
            COMPOSED="$PATH_COMPOSED/inner-pre-nestedN.wasm"
            ;;
        --fanin)
            COMPOSED="$PATH_FIXTURES/fanin.wasm"
            ;;
        --fanin1)
            COMPOSED="$PATH_COMPOSED/fanin1.wasm"
            ;;
        --faninN)
            COMPOSED="$PATH_COMPOSED/faninN.wasm"
            ;;
        --fanin-all1)
            COMPOSED="$PATH_COMPOSED/fanin-all1.wasm"
            ;;
        --fanin-allN)
            COMPOSED="$PATH_COMPOSED/fanin-allN.wasm"
            ;;
        --block1)
            COMPOSED="$PATH_COMPOSED/block1.wasm"
            ;;
        --blockN)
            COMPOSED="$PATH_COMPOSED/blockN.wasm"
            ;;
        --nonblock1)
            COMPOSED="$PATH_COMPOSED/block1.wasm"
            ;;
        --nonblockN)
            COMPOSED="$PATH_COMPOSED/blockN.wasm"
            ;;
        --tier2)
            COMPOSED="$PATH_COMPOSED/tier2.wasm"
            ;;
        *)
            log_error "Unknown option: $1"
            print_usage
            exit 1
            ;;
    esac

    viz "$COMPOSED" "$1"
}

# -----------------------------------------------------------------------------
# Build the component
# -----------------------------------------------------------------------------

build() {
    check_env
    build_component "service_a"     "$PATH_HANDLERS/service_a"    "service_a"
    build_component "service_b"     "$PATH_HANDLERS/service_b"    "service_b"
    build_component "service_c"     "$PATH_HANDLERS/service_c"    "service_c"

    build_component "printer_mdl"      "$PATH_PROXY_MDL/printer_mdl"      "printer_mdl"
    build_component "blocker_mdl"      "$PATH_PROXY_MDL/blocker_mdl"      "blocker_mdl"
    build_component "typed_logger_mdl" "$PATH_PROXY_MDL/typed_logger_mdl" "typed_logger_mdl"

    build_component "adder"           "$PATH_FAN_IN/adder"            "adder"
    build_component "adder_async"     "$PATH_FAN_IN/adder_async"      "adder_async"
    build_component "messenger"       "$PATH_FAN_IN/messenger"       "messenger"
    build_component "messenger_async" "$PATH_FAN_IN/messenger_async" "messenger_async"
    build_component "printer1"        "$PATH_FAN_IN/printer1"         "printer1"
    build_component "printer1_async"  "$PATH_FAN_IN/printer1_async"   "printer1_async"
    build_component "printer_n"       "$PATH_FAN_IN/printer_n"        "printer_n"
    build_component "shapes"          "$PATH_FAN_IN/shapes"           "shapes"
    build_component "shapes_handles"  "$PATH_FAN_IN/shapes_handles"   "shapes_handles"
    build_component "service"         "$PATH_FAN_IN/service"          "service"

    # Copy built components to fixtures/ for checked-in test data
    update_fixtures

    # Generate base compositions and copy them to fixtures/ so that downstream
    # configs (e.g. --blockN, --fanin1, --inner-nested1) can run from the
    # fixtures without rebuilding (see --skip-build).
    #
    # Order matters: `compose_nested` reads `$PATH_FIXTURES/chained.wasm`,
    # so chain must be composed AND copied into fixtures before the
    # nested compose runs.
    compose_chain
    cp "$PATH_COMPOSED/chained.wasm" "$PATH_FIXTURES/chained.wasm"
    compose_nested
    cp "$PATH_COMPOSED/nested.wasm"  "$PATH_FIXTURES/nested.wasm"
    compose_fanin
    cp "$PATH_COMPOSED/fanin.wasm"   "$PATH_FIXTURES/fanin.wasm"
}

# Copy all built .comp.wasm files to the fixtures/ directory.
# This directory is checked into git so that downstream projects (e.g. splicer
# via git submodule) can run integration tests without rebuilding from source.
update_fixtures() {
    log_info "Copying built components to $PATH_FIXTURES/..."
    mkdir -p "$PATH_FIXTURES"
    for f in "$PATH_WASI_TARGET"/*.comp.wasm; do
        cp "$f" "$PATH_FIXTURES/"
    done
    log_success "Fixtures updated in $PATH_FIXTURES/"
}

run_tests() {
    implemented_options=( \
      "--single" "--multiple" \
      "--chain" "--chain1" "--chainN" \
      "--nested" "--inner-nested1" "--inner-nestedN" \
      "--pre-nested1" "--pre-nestedN" \
      "--inner+pre-nested1" "--inner+pre-nestedN" \
      "--fanin" \
      "--fanin1" "--faninN" \
      "--fanin-all1" "--fanin-allN" \
      "--block1" "--blockN" "--nonblock1" "--nonblockN" \
    )
    log_info "Running all different configurations, these should all execute successfully!\n"

    check_env
    if [ "$SKIP_BUILD" -eq 0 ]; then
        build
    fi

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
# Strip --skip-build from "$@" before positional parsing so callers can pass it
# in any position without shifting CMD/OPT.
SKIP_BUILD=0
FILTERED_ARGS=()
for arg in "$@"; do
    if [ "$arg" = "--skip-build" ]; then
        SKIP_BUILD=1
    else
        FILTERED_ARGS+=("$arg")
    fi
done
set -- "${FILTERED_ARGS[@]}"

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
    viz)
        check_env
        viz_composition "$OPT"
        ;;
    all)
        if [ "$SKIP_BUILD" -eq 0 ]; then
            build
        fi
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
