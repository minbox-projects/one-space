#!/usr/bin/env bash
# Common platform checks and runtime dependency installation for the Cloud Agent.
set -euo pipefail

OS_RELEASE_FILE="${OS_RELEASE:-/etc/os-release}"
BOOTSTRAP_PHASE="${BOOTSTRAP_PHASE:-all}"

platform_fail() {
    printf 'bootstrap: %s\n' "$1" >&2
    exit "$2"
}

has_command() {
    command -v "$1" >/dev/null 2>&1
}

opencode_present() {
    has_command opencode && return 0
    [ -x /root/.opencode/bin/opencode ] && return 0
    [ -x /usr/local/bin/opencode ] && return 0
    [ -x /usr/bin/opencode ] && return 0
    return 1
}

runtime_missing() {
    local command_name
    for command_name in python3 git tmux gh; do
        if ! has_command "$command_name"; then
            return 0
        fi
    done
    if ! opencode_present; then
        return 0
    fi
    return 1
}

check_common_platform() {
    if [ "$(uname -s 2>/dev/null || true)" != "Linux" ]; then
        platform_fail "platform_linux_required" 10
    fi
    if [ "$(id -u 2>/dev/null || true)" != "0" ]; then
        platform_fail "platform_root_required" 11
    fi
    if ! has_command systemctl; then
        platform_fail "platform_systemd_required" 12
    fi
    if ! systemctl --version >/dev/null 2>&1; then
        platform_fail "platform_systemd_required" 12
    fi
    case "$(uname -m 2>/dev/null || true)" in
        x86_64|amd64|aarch64|arm64) ;;
        *) platform_fail "platform_arch_unsupported" 13 ;;
    esac
}

check_distribution() {
    if [ ! -r "$OS_RELEASE_FILE" ]; then
        platform_fail "platform_distro_unsupported" 14
    fi
    # shellcheck disable=SC1090
    . "$OS_RELEASE_FILE"
    case "${ID:-}:${VERSION_ID:-}" in
        ubuntu:22.04|ubuntu:24.04|debian:12) ;;
        *) platform_fail "platform_distro_unsupported" 14 ;;
    esac
}

apt_update() {
    if ! DEBIAN_FRONTEND=noninteractive apt-get update -y; then
        platform_fail "dependencies_apt_update_failed" 20
    fi
}

apt_install() {
    if ! DEBIAN_FRONTEND=noninteractive apt-get install -y "$@"; then
        platform_fail "dependencies_apt_install_failed" 20
    fi
}

install_github_cli() {
    local apt_arch keyring_dir sources_file
    case "$(uname -m)" in
        x86_64|amd64) apt_arch="amd64" ;;
        aarch64|arm64) apt_arch="arm64" ;;
        *) platform_fail "platform_arch_unsupported" 13 ;;
    esac

    keyring_dir="$(mktemp -d)"
    sources_file="${keyring_dir}/github-cli.list"
    if ! curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
        | tee "${keyring_dir}/githubcli-archive-keyring.gpg" >/dev/null; then
        platform_fail "dependencies_gh_key_failed" 20
    fi
    chmod 644 "${keyring_dir}/githubcli-archive-keyring.gpg"
    printf 'deb [arch=%s signed-by=%s] https://cli.github.com/packages stable main\n' \
        "$apt_arch" "${keyring_dir}/githubcli-archive-keyring.gpg" > "$sources_file"

    if ! DEBIAN_FRONTEND=noninteractive apt-get \
        -o Dir::Etc::sourcelist="$sources_file" \
        -o Dir::Etc::sourceparts="-" \
        -o APT::Get::List-Cleanup="0" update -y; then
        platform_fail "dependencies_gh_source_failed" 20
    fi
    if ! DEBIAN_FRONTEND=noninteractive apt-get \
        -o Dir::Etc::sourcelist="$sources_file" \
        -o Dir::Etc::sourceparts="-" \
        -o APT::Get::List-Cleanup="0" install -y gh; then
        platform_fail "dependencies_gh_install_failed" 20
    fi
    if ! has_command gh; then
        platform_fail "dependencies_gh_verify_failed" 20
    fi
    if ! gh --version >/dev/null 2>&1; then
        platform_fail "dependencies_gh_verify_failed" 20
    fi
}

install_opencode() {
    printf 'Installing OpenCode 1.19.5 (--version 1.19.5 --no-modify-path)\n'
    if ! curl -fsSL https://opencode.ai/install | bash -s -- --version 1.19.5 --no-modify-path; then
        platform_fail "dependencies_opencode_install_failed" 20
    fi
}

install_dependencies() {
    local packages=""
    local command_name
    check_common_platform
    if ! runtime_missing; then
        return 0
    fi
    check_distribution

    for command_name in python3 git tmux; do
        if ! has_command "$command_name"; then
            packages="$packages $command_name"
        fi
    done
    packages="$packages curl ca-certificates tar"
    # shellcheck disable=SC2086
    apt_update
    # shellcheck disable=SC2086
    apt_install $packages

    if ! has_command gh; then
        install_github_cli
    fi
    if ! opencode_present; then
        install_opencode
    fi
}

case "$BOOTSTRAP_PHASE" in
    platform)
        check_common_platform
        if runtime_missing; then
            check_distribution
        fi
        ;;
    dependencies)
        install_dependencies
        ;;
    *)
        check_common_platform
        if runtime_missing; then
            check_distribution
            install_dependencies
        fi
        ;;
esac
