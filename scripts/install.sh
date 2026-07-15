#!/usr/bin/env bash
set -euo pipefail

APP_ID="io.github.pang.RjSupplicantGui"
SERVICE_NAME="rjsupplicant.service"

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"
APP_DIR="${DATA_HOME}/rjsupplicant"
DESKTOP_DIR="${DATA_HOME}/applications"
DESKTOP_FILE="${DESKTOP_DIR}/${APP_ID}.desktop"
OLD_DESKTOP_FILE="${DESKTOP_DIR}/rjsupplicant.desktop"
ICON_DIR="${DATA_HOME}/icons/hicolor/scalable/apps"
ICON_FILE="${ICON_DIR}/${APP_ID}.svg"
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
CONFIG_DIR="${CONFIG_HOME}/rjsupplicant-gui"
SYSTEMD_SYSTEM_DIR="${RJSUPPLICANT_SYSTEMD_DIR:-/etc/systemd/system}"
SERVICE_FILE="${SYSTEMD_SYSTEM_DIR}/${SERVICE_NAME}"
LIBEXEC_DIR="${RJSUPPLICANT_LIBEXEC_DIR:-/usr/lib/rjsupplicant-gui}"
HELPER_FILE="${LIBEXEC_DIR}/rjsupplicant-helper"
ROOT_CLIENT_DIR="${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR:-/usr/lib/rjsupplicant}"
ROOT_WRAPPER_FILE="${LIBEXEC_DIR}/rjsupplicant"
POLICY_DIR="${RJSUPPLICANT_POLICY_DIR:-/usr/share/polkit-1/actions}"
POLICY_FILE="${POLICY_DIR}/${APP_ID}.policy"

log() {
  printf '[rjsupplicant-gui] %s\n' "$*"
}

die() {
  printf '[rjsupplicant-gui] ERROR: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<EOF
用法：scripts/install.sh [选项]

无选项        安装或升级 rjsupplicant-gui
--uninstall   停止认证服务并移除已安装组件（保留用户设置）
-h, --help    显示此帮助
EOF
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

validate_path_overrides() {
  local name
  local override_used=0
  for name in \
    RJSUPPLICANT_SYSTEMD_DIR \
    RJSUPPLICANT_LIBEXEC_DIR \
    RJSUPPLICANT_PRIVILEGED_CLIENT_DIR \
    RJSUPPLICANT_POLICY_DIR; do
    if [[ -v "${name}" && -n "${!name}" ]]; then
      override_used=1
    fi
  done
  if [[ "${override_used}" == "0" ]]; then
    return
  fi
  [[ "${RJSUPPLICANT_TEST_MODE:-0}" == "1" ]] ||
    die "系统路径覆盖仅允许隔离回归测试使用。"

  local path
  for path in "${SYSTEMD_SYSTEM_DIR}" "${LIBEXEC_DIR}" "${ROOT_CLIENT_DIR}" "${POLICY_DIR}"; do
    [[ "${path}" == /tmp/* ]] || die "测试路径必须位于 /tmp：${path}"
  done
}

current_arch_name() {
  if [[ "$(getconf LONG_BIT)" == "64" ]]; then
    printf '%s\n' "x64"
  else
    printf '%s\n' "x86"
  fi
}

privileged_client_ready() {
  local arch
  arch="$(current_arch_name)"
  [[ -x "${HELPER_FILE}" && -x "${ROOT_WRAPPER_FILE}" && -x "${ROOT_CLIENT_DIR}/${arch}/rjsupplicant" ]]
}

legacy_client_ready() {
  local arch
  arch="$(current_arch_name)"
  [[ -x "${BIN_DIR}/rjsupplicant" && -x "${APP_DIR}/${arch}/rjsupplicant" ]]
}

install_system_deps() {
  if [[ "${SKIP_SYSTEM_DEPS:-0}" == "1" ]]; then
    log "跳过系统依赖安装。"
    return
  fi

  if need_cmd pacman; then
    log "安装/确认 Arch Linux 依赖。"
    sudo pacman -S --needed rust gtk4 libadwaita polkit desktop-file-utils unzip
    return
  fi

  log "未检测到 pacman；请手动安装 Rust、GTK4、libadwaita、polkit、desktop-file-utils、unzip。"
}

find_official_zip() {
  if [[ -n "${RJSUPPLICANT_ZIP:-}" && -f "${RJSUPPLICANT_ZIP}" ]]; then
    printf '%s\n' "${RJSUPPLICANT_ZIP}"
    return
  fi

  local candidate
  for candidate in \
    "${ROOT_DIR}"/RG_Supplicant_For_Linux*.zip \
    "${ROOT_DIR}"/rjsupplicant*.zip \
    "${HOME}"/Downloads/RG_Supplicant_For_Linux*.zip \
    "${HOME}"/Downloads/rjsupplicant*.zip; do
    if [[ -f "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return
    fi
  done
}

build_binaries() {
  log "构建 GUI 和特权 helper。"
  cargo build --release --manifest-path "${ROOT_DIR}/Cargo.toml"
}

install_privileged_helper() {
  log "安装 root-owned helper 和 polkit policy。"
  sudo install -D -m 755 "${ROOT_DIR}/target/release/rjsupplicant-helper" "${HELPER_FILE}"
  sudo install -D -m 644 "${ROOT_DIR}/data/${APP_ID}.policy" "${POLICY_FILE}"
}

install_official_client() {
  local zip_path
  zip_path="$(find_official_zip || true)"
  if [[ -z "${zip_path}" ]]; then
    if privileged_client_ready || legacy_client_ready; then
      log "官方客户端已存在，跳过安装。"
      return
    fi

    log "未找到官方客户端 zip，GUI 会先安装。"
    log "后续可把 RG_Supplicant_For_Linux*.zip 放到 ~/Downloads 后重新运行 scripts/install.sh。"
    return
  fi

  log "通过 root-owned helper 安装官方客户端：${zip_path}"
  sudo "${HELPER_FILE}" install-client "$(realpath "${zip_path}")"
}

install_gui() {
  mkdir -p "${BIN_DIR}" "${DESKTOP_DIR}" "${ICON_DIR}"
  install -m 755 "${ROOT_DIR}/target/release/rjsupplicant-gui" "${BIN_DIR}/rjsupplicant-gui"
  install -m 644 "${ROOT_DIR}/data/${APP_ID}.svg" "${ICON_FILE}"

  sed "s#^Exec=.*#Exec=${BIN_DIR}/rjsupplicant-gui#" \
    "${ROOT_DIR}/data/${APP_ID}.desktop" >"${DESKTOP_FILE}"
  chmod 644 "${DESKTOP_FILE}"

  rm -f "${OLD_DESKTOP_FILE}"
  if need_cmd update-desktop-database; then
    update-desktop-database "${DESKTOP_DIR}" || true
  fi
  if need_cmd gtk-update-icon-cache; then
    gtk-update-icon-cache -f -t "${DATA_HOME}/icons/hicolor" || true
  fi
}

uninstall_service() {
  if [[ ! -e "${SERVICE_FILE}" && ! -L "${SERVICE_FILE}" ]]; then
    log "未发现 systemd 服务，跳过系统服务清理。"
    return
  fi

  log "停止并移除 systemd 服务：${SERVICE_FILE}"
  if need_cmd systemctl; then
    sudo systemctl disable --now "${SERVICE_NAME}" || die "无法停止 ${SERVICE_NAME}，未继续删除服务文件。"
  fi
  sudo rm -f "${SERVICE_FILE}"
  if need_cmd systemctl; then
    sudo systemctl daemon-reload
    sudo systemctl reset-failed "${SERVICE_NAME}" >/dev/null 2>&1 || true
  fi
}

disconnect_running_client() {
  if ! need_cmd pgrep || ! pgrep -x rjsupplicant >/dev/null 2>&1; then
    return
  fi
  if privileged_client_ready; then
    log "通过 root-owned helper 断开仍在运行的认证进程。"
    sudo "${HELPER_FILE}" disconnect || die "无法断开认证进程，未继续删除客户端文件。"
    return
  fi
  if [[ ! -x "${BIN_DIR}/rjsupplicant" ]]; then
    log "检测到认证进程仍在运行，但 wrapper 不可用；请手动停止该进程。"
    return
  fi

  log "断开仍在运行的手动认证进程。"
  sudo "${BIN_DIR}/rjsupplicant" -q || die "无法断开认证进程，未继续删除客户端文件。"
}

uninstall_privileged_files() {
  if [[ ! -e "${HELPER_FILE}" && ! -e "${ROOT_CLIENT_DIR}" && ! -e "${POLICY_FILE}" ]]; then
    return
  fi
  log "移除 root-owned helper、客户端和 polkit policy。"
  sudo rm -rf "${ROOT_CLIENT_DIR}" "${LIBEXEC_DIR}"
  sudo rm -f "${POLICY_FILE}"
}

uninstall_user_files() {
  log "移除 GUI、官方客户端、桌面入口和图标。"
  rm -f \
    "${BIN_DIR}/rjsupplicant-gui" \
    "${BIN_DIR}/rjsupplicant" \
    "${DESKTOP_FILE}" \
    "${OLD_DESKTOP_FILE}" \
    "${ICON_FILE}"
  rm -rf "${APP_DIR}"

  if need_cmd update-desktop-database && [[ -d "${DESKTOP_DIR}" ]]; then
    update-desktop-database "${DESKTOP_DIR}" || true
  fi
  if need_cmd gtk-update-icon-cache && [[ -d "${DATA_HOME}/icons/hicolor" ]]; then
    gtk-update-icon-cache -f -t "${DATA_HOME}/icons/hicolor" || true
  fi
}

uninstall() {
  log "卸载将中断当前有线认证连接。"
  uninstall_service
  disconnect_running_client
  uninstall_privileged_files
  uninstall_user_files

  log "卸载完成。"
  if [[ -d "${CONFIG_DIR}" ]]; then
    log "已保留用户设置：${CONFIG_DIR}"
    log "如需彻底清除，可手动删除该目录。"
  fi
}

main() {
  [[ -n "${HOME:-}" ]] || die "HOME 为空，无法确定用户安装目录。"
  validate_path_overrides

  case "${1:-}" in
    "")
      ;;
    --uninstall)
      [[ "$#" -eq 1 ]] || die "--uninstall 不接受其他参数。"
      uninstall
      return
      ;;
    -h | --help)
      [[ "$#" -eq 1 ]] || die "帮助选项不接受其他参数。"
      usage
      return
      ;;
    *)
      die "未知选项：$1（使用 --help 查看用法）"
      ;;
  esac

  install_system_deps
  build_binaries
  install_privileged_helper
  install_official_client
  install_gui

  log "安装完成。"
  log "应用入口：锐捷有线认证"
  log "GUI：${BIN_DIR}/rjsupplicant-gui"
  log "特权 helper：${HELPER_FILE}"
  log "官方客户端：${ROOT_CLIENT_DIR}"
}

main "$@"
