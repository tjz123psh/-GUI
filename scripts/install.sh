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

log() {
  printf '[rjsupplicant-gui] %s\n' "$*"
}

die() {
  printf '[rjsupplicant-gui] ERROR: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

current_arch_dir() {
  if [[ "$(getconf LONG_BIT)" == "64" ]]; then
    printf '%s\n' "${APP_DIR}/x64"
  else
    printf '%s\n' "${APP_DIR}/x86"
  fi
}

official_client_ready() {
  local arch_dir
  arch_dir="$(current_arch_dir)"
  [[ -x "${BIN_DIR}/rjsupplicant" && -x "${arch_dir}/rjsupplicant" ]]
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

install_official_client() {
  mkdir -p "${BIN_DIR}" "${DATA_HOME}"

  local zip_path
  zip_path="$(find_official_zip || true)"
  if [[ -z "${zip_path}" ]]; then
    if official_client_ready; then
      log "官方客户端已存在，跳过安装。"
      return
    fi

    log "未找到官方客户端 zip，GUI 会先安装。"
    log "后续可把 RG_Supplicant_For_Linux*.zip 放到 ~/Downloads 后重新运行 scripts/install.sh。"
    return
  fi

  log "安装官方客户端：${zip_path}"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "${tmp_dir}"' RETURN

  unzip -q "${zip_path}" -d "${tmp_dir}"

  local extracted="${tmp_dir}/rjsupplicant"
  [[ -d "${extracted}" ]] || die "zip 内未找到 rjsupplicant 目录。"

  rm -rf "${APP_DIR}"
  cp -a "${extracted}" "${APP_DIR}"
  chmod +x "${APP_DIR}/x64/rjsupplicant" "${APP_DIR}/x86/rjsupplicant" 2>/dev/null || true

  cat >"${BIN_DIR}/rjsupplicant" <<EOF
#!/usr/bin/env bash
set -euo pipefail
app_dir="${APP_DIR}"
arch_dir="\${app_dir}/x64"
if [[ "\$(getconf LONG_BIT)" != "64" ]]; then
  arch_dir="\${app_dir}/x86"
fi
cd "\${arch_dir}"
export LD_LIBRARY_PATH="\${arch_dir}/lib\${LD_LIBRARY_PATH:+:\${LD_LIBRARY_PATH}}"
exec "\${arch_dir}/rjsupplicant" "\$@"
EOF
  chmod 755 "${BIN_DIR}/rjsupplicant"
}

install_service() {
  local service_tmp
  local arch_dir
  arch_dir="$(current_arch_dir)"

  if ! official_client_ready; then
    log "官方客户端未就绪，跳过 systemd 服务安装。"
    log "放入官方客户端 zip 后重新运行 scripts/install.sh，会自动生成 ${SERVICE_NAME}。"
    return
  fi

  service_tmp="$(mktemp)"
  cat >"${service_tmp}" <<EOF
[Unit]
Description=Ruijie RG-SU wired authentication client
After=NetworkManager.service network-online.target
Wants=NetworkManager.service network-online.target

[Service]
Type=simple
ExecStart=${BIN_DIR}/rjsupplicant -a 1 -d 1
ExecStop=${BIN_DIR}/rjsupplicant -q
Restart=on-failure
RestartSec=10
WorkingDirectory=${arch_dir}

[Install]
WantedBy=multi-user.target
EOF

  log "安装 systemd 服务：/etc/systemd/system/${SERVICE_NAME}"
  sudo install -m 644 "${service_tmp}" "/etc/systemd/system/${SERVICE_NAME}"
  rm -f "${service_tmp}"
  sudo systemctl daemon-reload
}

install_gui() {
  log "构建 GUI。"
  cargo build --release --manifest-path "${ROOT_DIR}/Cargo.toml"

  mkdir -p "${BIN_DIR}" "${DESKTOP_DIR}"
  install -m 755 "${ROOT_DIR}/target/release/rjsupplicant-gui" "${BIN_DIR}/rjsupplicant-gui"

  sed "s#^Exec=.*#Exec=${BIN_DIR}/rjsupplicant-gui#" \
    "${ROOT_DIR}/data/${APP_ID}.desktop" >"${DESKTOP_FILE}"
  chmod 644 "${DESKTOP_FILE}"

  rm -f "${OLD_DESKTOP_FILE}"
  if need_cmd update-desktop-database; then
    update-desktop-database "${DESKTOP_DIR}" || true
  fi
}

main() {
  [[ -n "${HOME:-}" ]] || die "HOME 为空，无法确定用户安装目录。"

  install_system_deps
  install_official_client
  install_service
  install_gui

  log "安装完成。"
  log "应用入口：锐捷有线认证"
  log "GUI：${BIN_DIR}/rjsupplicant-gui"
  log "官方客户端 wrapper：${BIN_DIR}/rjsupplicant"
}

main "$@"
