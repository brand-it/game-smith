#!/usr/bin/env bash
set -euo pipefail

# Local development setup for game-smith
# Checks and installs system dependencies for the desktop feature

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${GREEN}[setup]${NC} $*"; }
warn()  { echo -e "${YELLOW}[warn]${NC} $*"; }
error() { echo -e "${RED}[error]${NC} $*"; }
step()  { echo -e "${CYAN}[step]${NC} $*"; }

# OS / distro detection
OS_TYPE=""
REQUIRED_PACKAGES=""
USE_BREW=false

detect_os() {
    # Prefer Homebrew if available (works on macOS and Linux)
    if command -v brew &>/dev/null; then
        USE_BREW=true
        info "Detected Homebrew"
        REQUIRED_PACKAGES="gtk+3 libayatana-appindicator"
        return
    fi

    if [[ "$OSTYPE" == "darwin"* ]]; then
        warn "macOS detected but Homebrew not found."
        warn "Install it from https://brew.sh/, then run this script again."
        OS_TYPE="macos"
        return
    fi

    if [[ ! -f /etc/os-release ]]; then
        error "/etc/os-release not found."
        exit 1
    fi

    . /etc/os-release
    local id="${ID:-}"
    local id_like="${ID_LIKE:-}"

    if [[ "$id" == "fedora" ]] || [[ "$id_like" == *"fedora"* ]]; then
        info "Detected Fedora"
        OS_TYPE="fedora"
        REQUIRED_PACKAGES="gtk3-devel libappindicator-gtk3-devel"
    elif [[ "$id" == "ubuntu" || "$id" == "debian" || "$id_like" == *"debian"* ]]; then
        info "Detected Debian/Ubuntu"
        OS_TYPE="debian"
        REQUIRED_PACKAGES="libgtk-3-dev libappindicator3-dev"
    elif [[ "$id" == "arch" || "$id_like" == *"arch"* ]]; then
        info "Detected Arch Linux"
        OS_TYPE="arch"
        REQUIRED_PACKAGES="gtk3 libappindicator-gtk3"
    elif [[ "$id" == "opensuse-tumbleweed" || "$id" == "sles" || "$id_like" == *"suse"* ]]; then
        info "Detected openSUSE"
        OS_TYPE="suse"
        REQUIRED_PACKAGES="gtk3-devel libappindicator3-devel"
    else
        warn "Unknown distribution: $id"
        OS_TYPE="unknown"
    fi
}

# Prompt for confirmation before running a command
confirm() {
    local cmd="$1"
    echo ""
    warn "This requires elevated privileges:"
    echo "  $cmd"
    echo ""
    read -r -p "  Continue? [y/N] " reply
    case "$reply" in
        [yY][eE][sS]|[yY]) return 0 ;;
        *) return 1 ;;
    esac
}

install_packages() {
    if [[ "$USE_BREW" == true ]]; then
        info "Installing via Homebrew (no root required)..."
        brew install $REQUIRED_PACKAGES || {
            error "Failed to install packages via brew."
            exit 1
        }
    else
        local pkg_mgr=""
        case "$OS_TYPE" in
            fedora)   pkg_mgr="sudo dnf install -y" ;;
            debian)   pkg_mgr="sudo apt install -y" ;;
            arch)     pkg_mgr="sudo pacman -S --noconfirm" ;;
            suse)     pkg_mgr="sudo zypper install -y" ;;
            *)        return ;;
        esac

        local cmd="$pkg_mgr $REQUIRED_PACKAGES"
        if confirm "$cmd"; then
            eval "$cmd" || {
                error "Package installation failed."
                exit 1
            }
        else
            warn "Skipped package installation."
            warn "Install manually and run this script again:"
            echo "  $cmd"
            exit 1
        fi
    fi
}

check_dep() {
    local name="$1"
    local check_cmd="$2"
    if eval "$check_cmd" &>/dev/null; then
        info "  ✓ $name"
        return 0
    else
        warn "  ✗ $name"
        return 1
    fi
}

check_dependencies() {
    local missing=false

    step "Checking Rust toolchain..."
    if ! check_dep "Rust stable" "rustc --version"; then
        warn "Install Rust from https://rustup.rs/"
        missing=true
    fi

    step "Checking desktop feature dependencies..."
    check_dep "GTK3 dev headers" "pkg-config --exists gtk+-3.0" || missing=true
    check_dep "libappindicator" "pkg-config --exists libappindicator-gtk3" || missing=true

    if [[ "$missing" == false ]]; then
        return 0
    fi
    return 1
}

main() {
    info "Setting up game-smith development environment..."
    echo ""

    detect_os
    echo ""

    # --check: exit after dependency verification
    if [[ "${1:-}" == "--check" ]]; then
        if check_dependencies; then
            info "All dependencies satisfied."
        else
            echo ""
            error "Some dependencies are missing."
            info "Run without --check to install them:"
            echo "  ./scripts/setup.sh"
            exit 1
        fi
        return 0
    fi

    if check_dependencies; then
        info "All dependencies satisfied."
        echo ""
        info "Build the project with:"
        echo "  cargo run -- start"
        return 0
    fi

    # Dependencies missing — offer to install
    if [[ -n "$REQUIRED_PACKAGES" ]]; then
        echo ""
        step "Installing missing dependencies..."
        install_packages
    else
        error "Cannot determine packages for this platform."
        error "Install GTK3 and libappindicator manually."
        exit 1
    fi

    echo ""
    info "Setup complete!"
    info "Build the project with:"
    echo "  cargo run -- start"
}

main "$@"
