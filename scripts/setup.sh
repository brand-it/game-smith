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
        install_brew_packages
    else
        install_system_packages
    fi
}

install_brew_packages() {
    local packages_to_install=()

    for pkg in $REQUIRED_PACKAGES; do
        if ! brew list --formula "$pkg" &>/dev/null; then
            packages_to_install+=("$pkg")
        fi
    done

    if [[ ${#packages_to_install[@]} -eq 0 ]]; then
        info "All Homebrew packages already installed."
        return 0
    fi

    info "Installing missing Homebrew packages: ${packages_to_install[*]}"
    brew install "${packages_to_install[@]}" 2>/dev/null || {
        error "Failed to install packages via brew."
        exit 1
    }
}

install_system_packages() {
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
}

# Compute a PKG_CONFIG_PATH that includes Homebrew's prefix if available.
BREW_PC_PATH=""
resolve_pkg_config_path() {
    if [[ -n "$BREW_PC_PATH" ]]; then
        return
    fi
    local pc_path="${PKG_CONFIG_PATH:-}"
    if [[ "$USE_BREW" == true ]] && command -v brew &>/dev/null; then
        local brew_prefix
        brew_prefix="$(brew --prefix 2>/dev/null)" || true
        if [[ -n "$brew_prefix" ]] && [[ ":$pc_path:" != *":$brew_prefix:"* ]]; then
            pc_path="${pc_path:+$pc_path:}${brew_prefix}/lib/pkgconfig"
        fi
    fi
    BREW_PC_PATH="$pc_path"
}

# Check a dependency via pkg-config, using Homebrew's prefix if available.
# Prints the detected version next to the check mark.
pkg_config_check() {
    local name="$1"
    local pkg_name="$2"
    resolve_pkg_config_path

    if PKG_CONFIG_PATH="$BREW_PC_PATH" pkg-config --exists "$pkg_name" 2>/dev/null; then
        local version
        version="$(PKG_CONFIG_PATH="$BREW_PC_PATH" pkg-config --modversion "$pkg_name" 2>/dev/null)" || version=""
        info "  ✓ $name ($version)"
        return 0
    else
        warn "  ✗ $name"
        return 1
    fi
}

# Check a dependency by running a command and extracting a version string.
# Usage: check_dep "Rust stable" "rustc --version"
check_dep() {
    local name="$1"
    local check_cmd="$2"
    if eval "$check_cmd" &>/dev/null; then
        local version
        version="$(eval "$check_cmd" 2>&1 | head -1 | sed -n 's/.* \([0-9]\+\.[0-9]\+\.[0-9]\+\).*/\1/p')" || version=""
        info "  ✓ $name ($version)"
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
    pkg_config_check "GTK3 dev headers" "gtk+-3.0" || missing=true
    pkg_config_check "libappindicator" "ayatana-appindicator3-0.1" || missing=true

    step "Checking Rust dev tools..."
    check_dep "rust-analyzer" "rust-analyzer --version" || missing=true

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

        # Warn about outdated packages
        if [[ "$USE_BREW" == true ]]; then
            local outdated
            outdated="$(brew outdated $REQUIRED_PACKAGES 2>/dev/null)" || true
            if [[ -n "$outdated" ]]; then
                echo ""
                warn "New package versions available. Upgrade with:"
                echo "  brew upgrade $REQUIRED_PACKAGES"
            fi
        fi

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

    # Install rust-analyzer if missing
    if ! rustup component list --installed 2>/dev/null | grep -q rust-analyzer; then
        step "Installing rust-analyzer..."
        rustup component add rust-analyzer
        info "  ✓ rust-analyzer"
    fi

    # Verify everything is satisfied after installation
    echo ""
    step "Verifying installation..."
    resolve_pkg_config_path
    local verify_failed=false
    local verify_msgs=()

    if ! PKG_CONFIG_PATH="$BREW_PC_PATH" pkg-config --exists gtk+-3.0 2>/dev/null; then
        verify_failed=true
        verify_msgs+=("GTK3 dev headers")
    fi
    if ! PKG_CONFIG_PATH="$BREW_PC_PATH" pkg-config --exists ayatana-appindicator3-0.1 2>/dev/null; then
        verify_failed=true
        verify_msgs+=("libappindicator")
    fi
    if ! rustup component list --installed 2>/dev/null | grep -q rust-analyzer; then
        verify_failed=true
        verify_msgs+=("rust-analyzer")
    fi

    if [[ "$verify_failed" == true ]]; then
        echo ""
        warn "pkg-config cannot find:"
        for msg in "${verify_msgs[@]}"; do
            echo "  - $msg"
        done
        echo ""
        info "Ensure PKG_CONFIG_PATH includes your package manager's pkg-config dir."
    fi

    # Warn about outdated packages
    if [[ "$USE_BREW" == true ]]; then
        local outdated
        outdated="$(brew outdated $REQUIRED_PACKAGES 2>/dev/null)" || true
        if [[ -n "$outdated" ]]; then
            echo ""
            warn "New package versions available. Upgrade with:"
            echo "  brew upgrade $REQUIRED_PACKAGES"
        fi
    fi

    echo ""
    info "Setup complete!"
    info "Build the project with:"
    echo "  cargo run -- start"
}

main "$@"
