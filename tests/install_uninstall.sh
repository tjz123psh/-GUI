#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_HOME="${HOME}"
HOST_CARGO_HOME="${CARGO_HOME:-${HOST_HOME}/.cargo}"
HOST_RUSTUP_HOME="${RUSTUP_HOME:-${HOST_HOME}/.rustup}"
TMP_DIR="$(mktemp -d)"
FAKE_CLIENT_PID=""
# 同时覆盖 TERM/INT/HUP：被 CI 超时或人工打断时，仅靠 EXIT  trap 要等前台
# cargo build 结束才执行，进程随即被 SIGKILL，会在 /tmp 留下数百 MB 临时树。
cleanup() {
  if [[ -n "${FAKE_CLIENT_PID}" ]]; then
    kill "${FAKE_CLIENT_PID}" 2>/dev/null || true
  fi
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT HUP INT TERM

HOME_DIR="${TMP_DIR}/home"
DATA_HOME="${TMP_DIR}/data"
CONFIG_HOME="${TMP_DIR}/config"
SYSTEMD_DIR="${TMP_DIR}/systemd"
FAKE_BIN="${TMP_DIR}/bin"
SUDO_LOG="${TMP_DIR}/sudo.log"
PACMAN_LOG="${TMP_DIR}/pacman.log"
PRIVILEGED_DIR="${TMP_DIR}/lib/rjsupplicant-gui"
ROOT_CLIENT_DIR="${TMP_DIR}/lib/rjsupplicant"
POLICY_DIR="${TMP_DIR}/polkit-actions"
export RJSUPPLICANT_TEST_MODE=1
# 构建隔离：私有 target 目录 + 不触发 cargo clean，避免回归测试删掉
# 开发者（或并行 Agent）工作树里的 target/；TMP_DIR 由 trap 统一清理。
export CARGO_TARGET_DIR="${TMP_DIR}/cargo-target"
export RJSUPPLICANT_KEEP_BUILD=1

mkdir -p \
  "${HOME_DIR}/.local/bin" \
  "${DATA_HOME}/rjsupplicant/x64" \
  "${DATA_HOME}/applications" \
  "${DATA_HOME}/icons/hicolor/scalable/apps" \
  "${CONFIG_HOME}/rjsupplicant-gui" \
  "${SYSTEMD_DIR}" \
  "${PRIVILEGED_DIR}" \
  "${ROOT_CLIENT_DIR}/x64" \
  "${POLICY_DIR}" \
  "${FAKE_BIN}"

touch \
  "${HOME_DIR}/.local/bin/rjsupplicant-gui" \
  "${HOME_DIR}/.local/bin/rjsupplicant" \
  "${DATA_HOME}/rjsupplicant/x64/rjsupplicant" \
  "${DATA_HOME}/applications/io.github.pang.RjSupplicantGui.desktop" \
  "${DATA_HOME}/applications/rjsupplicant.desktop" \
  "${DATA_HOME}/icons/hicolor/scalable/apps/io.github.pang.RjSupplicantGui.svg" \
  "${CONFIG_HOME}/rjsupplicant-gui/settings.conf" \
  "${SYSTEMD_DIR}/rjsupplicant.service" \
  "${PRIVILEGED_DIR}/rjsupplicant-helper" \
  "${PRIVILEGED_DIR}/rjsupplicant" \
  "${ROOT_CLIENT_DIR}/x64/rjsupplicant" \
  "${POLICY_DIR}/io.github.pang.RjSupplicantGui.policy"
chmod 755 \
  "${PRIVILEGED_DIR}/rjsupplicant-helper" \
  "${PRIVILEGED_DIR}/rjsupplicant" \
  "${ROOT_CLIENT_DIR}/x64/rjsupplicant"

cat >"${FAKE_BIN}/sudo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${SUDO_LOG}"
if [[ "${1:-}" == "systemctl" ]]; then
  if [[ "${FAIL_DISABLE:-0}" == "1" && "${2:-}" == "disable" ]]; then
    exit 1
  fi
  exit 0
fi
if [[ "${1:-}" == "${RJSUPPLICANT_LIBEXEC_DIR:-}/rjsupplicant-helper" ]]; then
  case "${2:-}" in
    install-client)
      if [[ "${FAIL_HELPER_INSTALL:-0}" == "1" || "${UNSAFE_ZIP:-0}" == "1" ]]; then
        exit 1
      fi
      mkdir -p \
        "${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR}/x64" \
        "${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR}/x86"
      touch \
        "${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR}/x64/rjsupplicant" \
        "${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR}/x86/rjsupplicant" \
        "${RJSUPPLICANT_LIBEXEC_DIR}/rjsupplicant"
      chmod 755 \
        "${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR}/x64/rjsupplicant" \
        "${RJSUPPLICANT_PRIVILEGED_CLIENT_DIR}/x86/rjsupplicant" \
        "${RJSUPPLICANT_LIBEXEC_DIR}/rjsupplicant"
      ;;
    disconnect) ;;
    *) exit 1 ;;
  esac
  exit 0
fi
exec "$@"
EOF
chmod 755 "${FAKE_BIN}/sudo"

cat >"${FAKE_BIN}/pacman" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
for argument in "$@"; do
  if [[ "${argument}" == "rust" ]]; then
    printf '已有 rustup 时安装器仍请求 pacman rust\n' >&2
    exit 1
  fi
done
printf '%s\n' "$*" >"${PACMAN_LOG}"
EOF
chmod 755 "${FAKE_BIN}/pacman"

cat >"${FAKE_BIN}/unzip" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-Z1" ]]; then
  if [[ "${UNSAFE_ZIP:-0}" == "1" ]]; then
    printf '../outside\n'
  else
    printf '%s\n' \
      'rjsupplicant/x64/rjsupplicant' \
      'rjsupplicant/x86/rjsupplicant' \
      'rjsupplicant/x64/lib/placeholder'
  fi
  exit 0
fi

destination=""
while (( $# > 0 )); do
  if [[ "$1" == "-d" ]]; then
    shift
    destination="${1:-}"
  fi
  shift
done
[[ -n "${destination}" ]]
mkdir -p \
  "${destination}/rjsupplicant/x64/lib" \
  "${destination}/rjsupplicant/x86"
touch \
  "${destination}/rjsupplicant/x64/rjsupplicant" \
  "${destination}/rjsupplicant/x86/rjsupplicant" \
  "${destination}/rjsupplicant/x64/lib/placeholder"
EOF
chmod 755 "${FAKE_BIN}/unzip"

HOME="${HOME_DIR}" \
XDG_DATA_HOME="${DATA_HOME}" \
XDG_CONFIG_HOME="${CONFIG_HOME}" \
RJSUPPLICANT_SYSTEMD_DIR="${SYSTEMD_DIR}" \
RJSUPPLICANT_LIBEXEC_DIR="${PRIVILEGED_DIR}" \
RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${ROOT_CLIENT_DIR}" \
RJSUPPLICANT_POLICY_DIR="${POLICY_DIR}" \
SUDO_LOG="${SUDO_LOG}" \
PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" --uninstall

for removed_path in \
  "${HOME_DIR}/.local/bin/rjsupplicant-gui" \
  "${HOME_DIR}/.local/bin/rjsupplicant" \
  "${DATA_HOME}/rjsupplicant" \
  "${DATA_HOME}/applications/io.github.pang.RjSupplicantGui.desktop" \
  "${DATA_HOME}/applications/rjsupplicant.desktop" \
  "${DATA_HOME}/icons/hicolor/scalable/apps/io.github.pang.RjSupplicantGui.svg" \
  "${SYSTEMD_DIR}/rjsupplicant.service" \
  "${PRIVILEGED_DIR}" \
  "${ROOT_CLIENT_DIR}" \
  "${POLICY_DIR}/io.github.pang.RjSupplicantGui.policy"; do
  if [[ -e "${removed_path}" || -L "${removed_path}" ]]; then
    printf '卸载后仍存在：%s\n' "${removed_path}" >&2
    exit 1
  fi
done

[[ -f "${CONFIG_HOME}/rjsupplicant-gui/settings.conf" ]]
grep -Fq 'systemctl disable --now rjsupplicant.service' "${SUDO_LOG}"
grep -Fq 'systemctl daemon-reload' "${SUDO_LOG}"
grep -Fq "rm -f ${SYSTEMD_DIR}/rjsupplicant.service" "${SUDO_LOG}"

FAILURE_HOME="${TMP_DIR}/failure-home"
FAILURE_DATA="${TMP_DIR}/failure-data"
FAILURE_SYSTEMD="${TMP_DIR}/failure-systemd"
mkdir -p "${FAILURE_HOME}/.local/bin" "${FAILURE_DATA}/rjsupplicant" "${FAILURE_SYSTEMD}"
touch \
  "${FAILURE_HOME}/.local/bin/rjsupplicant-gui" \
  "${FAILURE_SYSTEMD}/rjsupplicant.service"

if HOME="${FAILURE_HOME}" \
  XDG_DATA_HOME="${FAILURE_DATA}" \
  RJSUPPLICANT_SYSTEMD_DIR="${FAILURE_SYSTEMD}" \
  RJSUPPLICANT_LIBEXEC_DIR="${TMP_DIR}/failure-libexec" \
  RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${TMP_DIR}/failure-client" \
  RJSUPPLICANT_POLICY_DIR="${TMP_DIR}/failure-policy" \
  SUDO_LOG="${SUDO_LOG}" \
  FAIL_DISABLE=1 \
  PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" --uninstall >/dev/null 2>&1; then
  printf '服务停止失败时卸载未返回失败状态\n' >&2
  exit 1
fi
[[ -f "${FAILURE_SYSTEMD}/rjsupplicant.service" ]]
[[ -f "${FAILURE_HOME}/.local/bin/rjsupplicant-gui" ]]

INSTALL_HOME="${TMP_DIR}/install-home"
INSTALL_DATA="${TMP_DIR}/install data%quoted"
INSTALL_CONFIG="${TMP_DIR}/install-config"
INSTALL_SYSTEMD="${TMP_DIR}/install-systemd"
INSTALL_LIBEXEC="${TMP_DIR}/install-libexec"
INSTALL_ROOT_CLIENT="${TMP_DIR}/install-root-client"
INSTALL_POLICY="${TMP_DIR}/install-policy"
INSTALL_ZIP="${TMP_DIR}/official.zip"
mkdir -p \
  "${INSTALL_HOME}" \
  "${INSTALL_DATA}" \
  "${INSTALL_CONFIG}" \
  "${INSTALL_SYSTEMD}" \
  "${INSTALL_POLICY}"
touch "${INSTALL_ZIP}"

HOME="${INSTALL_HOME}" \
XDG_DATA_HOME="${INSTALL_DATA}" \
XDG_CONFIG_HOME="${INSTALL_CONFIG}" \
RJSUPPLICANT_SYSTEMD_DIR="${INSTALL_SYSTEMD}" \
RJSUPPLICANT_LIBEXEC_DIR="${INSTALL_LIBEXEC}" \
RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${INSTALL_ROOT_CLIENT}" \
RJSUPPLICANT_POLICY_DIR="${INSTALL_POLICY}" \
RJSUPPLICANT_ZIP="${INSTALL_ZIP}" \
SUDO_LOG="${SUDO_LOG}" \
PACMAN_LOG="${PACMAN_LOG}" \
CARGO_HOME="${HOST_CARGO_HOME}" \
RUSTUP_HOME="${HOST_RUSTUP_HOME}" \
PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" >/dev/null

[[ -x "${INSTALL_HOME}/.local/bin/rjsupplicant-gui" ]]
[[ -x "${INSTALL_LIBEXEC}/rjsupplicant-helper" ]]
[[ -x "${INSTALL_LIBEXEC}/rjsupplicant" ]]
[[ -x "${INSTALL_ROOT_CLIENT}/x64/rjsupplicant" ]]
[[ -f "${INSTALL_POLICY}/io.github.pang.RjSupplicantGui.policy" ]]
[[ ! -e "${INSTALL_SYSTEMD}/rjsupplicant.service" ]]
grep -Fq 'gtk4 libadwaita polkit desktop-file-utils unzip' "${PACMAN_LOG}"

HOME="${INSTALL_HOME}" \
XDG_DATA_HOME="${INSTALL_DATA}" \
XDG_CONFIG_HOME="${INSTALL_CONFIG}" \
RJSUPPLICANT_SYSTEMD_DIR="${INSTALL_SYSTEMD}" \
RJSUPPLICANT_LIBEXEC_DIR="${INSTALL_LIBEXEC}" \
RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${INSTALL_ROOT_CLIENT}" \
RJSUPPLICANT_POLICY_DIR="${INSTALL_POLICY}" \
RJSUPPLICANT_ZIP="${INSTALL_ZIP}" \
SKIP_SYSTEM_DEPS=1 \
FAIL_HELPER_INSTALL=1 \
SUDO_LOG="${SUDO_LOG}" \
CARGO_HOME="${HOST_CARGO_HOME}" \
RUSTUP_HOME="${HOST_RUSTUP_HOME}" \
PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" >/dev/null

# 覆盖边界说明：下面 UNSAFE_ZIP / ROLLBACK / FAIL_HELPER_INSTALL 三个用例走的是
# fake sudo —— install.sh 自己从不调用 unzip（解压发生在真实 root helper 内，
# 且用绝对 /usr/bin/unzip）。所以它们验证的是"sudo 步骤失败时 install.sh 以非零
# 退出且不继续删除文件"，并不验证 ZIP 条目净化与目录替换回滚；后两者的真实覆盖在
# src/client_install.rs 的单元测试（rejects_unsafe_archive_entries、
# rejects_zip_with_parent_directory_entry、rejects_symlinks_*、
# restores_previous_client_when_wrapper_install_fails）。
UNSAFE_HOME="${TMP_DIR}/unsafe-home"
UNSAFE_DATA="${TMP_DIR}/unsafe-data"
UNSAFE_SYSTEMD="${TMP_DIR}/unsafe-systemd"
mkdir -p "${UNSAFE_HOME}" "${UNSAFE_DATA}" "${UNSAFE_SYSTEMD}"
if HOME="${UNSAFE_HOME}" \
  XDG_DATA_HOME="${UNSAFE_DATA}" \
  RJSUPPLICANT_SYSTEMD_DIR="${UNSAFE_SYSTEMD}" \
  RJSUPPLICANT_LIBEXEC_DIR="${TMP_DIR}/unsafe-libexec" \
  RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${TMP_DIR}/unsafe-client" \
  RJSUPPLICANT_POLICY_DIR="${TMP_DIR}/unsafe-policy" \
  RJSUPPLICANT_ZIP="${INSTALL_ZIP}" \
  SKIP_SYSTEM_DEPS=1 \
  UNSAFE_ZIP=1 \
  SUDO_LOG="${SUDO_LOG}" \
  PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" >/dev/null 2>&1; then
  printf '不安全 ZIP 路径未被拒绝\n' >&2
  exit 1
fi
[[ ! -e "${TMP_DIR}/unsafe-client" ]]

ROLLBACK_HOME="${TMP_DIR}/rollback-home"
ROLLBACK_DATA="${TMP_DIR}/rollback-data"
ROLLBACK_SYSTEMD="${TMP_DIR}/rollback-systemd"
mkdir -p \
  "${ROLLBACK_HOME}" \
  "${ROLLBACK_DATA}" \
  "${TMP_DIR}/rollback-client" \
  "${ROLLBACK_SYSTEMD}"
touch "${TMP_DIR}/rollback-client/old-marker"
if HOME="${ROLLBACK_HOME}" \
  XDG_DATA_HOME="${ROLLBACK_DATA}" \
  RJSUPPLICANT_SYSTEMD_DIR="${ROLLBACK_SYSTEMD}" \
  RJSUPPLICANT_LIBEXEC_DIR="${TMP_DIR}/rollback-libexec" \
  RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${TMP_DIR}/rollback-client" \
  RJSUPPLICANT_POLICY_DIR="${TMP_DIR}/rollback-policy" \
  RJSUPPLICANT_ZIP="${INSTALL_ZIP}" \
  SKIP_SYSTEM_DEPS=1 \
  FAIL_HELPER_INSTALL=1 \
  SUDO_LOG="${SUDO_LOG}" \
  PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" >/dev/null 2>&1; then
  printf 'wrapper 安装失败时脚本未返回失败状态\n' >&2
  exit 1
fi
[[ -f "${TMP_DIR}/rollback-client/old-marker" ]]
[[ ! -e "${TMP_DIR}/rollback-client/x64/rjsupplicant" ]]

# 用户可写的旧版 wrapper 不得被 sudo 以 root 执行（提权边界）。
# CI 容器以 root 运行，新建文件天然 root-owned，"属主非 root"这一半判据在 CI 里
# 无法构造；因此这里用 0777（组/其他可写），两种身份下都必然触发同一句拒绝。
LEGACY_HOME="${TMP_DIR}/legacy-home"
LEGACY_DATA="${TMP_DIR}/legacy-data"
LEGACY_SYSTEMD="${TMP_DIR}/legacy-systemd"
LEGACY_LIBEXEC="${TMP_DIR}/legacy-libexec"
LEGACY_CLIENT="${TMP_DIR}/legacy-client"
LEGACY_POLICY="${TMP_DIR}/legacy-policy"
mkdir -p "${LEGACY_HOME}/.local/bin" "${LEGACY_DATA}/rjsupplicant/x64" "${LEGACY_SYSTEMD}"
cat >"${LEGACY_HOME}/.local/bin/rjsupplicant" <<'EOF'
#!/usr/bin/env bash
printf '旧版 wrapper 被以 root 执行\n' >&2
exit 1
EOF
chmod 777 "${LEGACY_HOME}/.local/bin/rjsupplicant"
touch "${LEGACY_DATA}/rjsupplicant/x64/rjsupplicant"
chmod 755 "${LEGACY_DATA}/rjsupplicant/x64/rjsupplicant"

# 让 pgrep -x rjsupplicant 稳定命中，不依赖宿主机是否恰好在认证。
cp "$(command -v sleep)" "${TMP_DIR}/rjsupplicant"
"${TMP_DIR}/rjsupplicant" 600 &
FAKE_CLIENT_PID=$!

LEGACY_OUTPUT="${TMP_DIR}/legacy-output"
if HOME="${LEGACY_HOME}" \
  XDG_DATA_HOME="${LEGACY_DATA}" \
  RJSUPPLICANT_SYSTEMD_DIR="${LEGACY_SYSTEMD}" \
  RJSUPPLICANT_LIBEXEC_DIR="${LEGACY_LIBEXEC}" \
  RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${LEGACY_CLIENT}" \
  RJSUPPLICANT_POLICY_DIR="${LEGACY_POLICY}" \
  SUDO_LOG="${SUDO_LOG}" \
  PATH="${FAKE_BIN}:${PATH}" \
  "${ROOT_DIR}/scripts/install.sh" --uninstall >"${LEGACY_OUTPUT}" 2>&1; then
  printf '用户可写的旧版 wrapper 被接受为 root 执行目标\n' >&2
  cat "${LEGACY_OUTPUT}" >&2
  exit 1
fi
if grep -Fq "sudo ${LEGACY_HOME}/.local/bin/rjsupplicant" "${SUDO_LOG}"; then
  printf '安装脚本把用户可写路径交给 sudo 执行\n' >&2
  exit 1
fi
grep -Fq '不以 root 执行' "${LEGACY_OUTPUT}" || {
  printf '拒绝执行用户可写 wrapper 时未给出可操作说明\n' >&2
  cat "${LEGACY_OUTPUT}" >&2
  exit 1
}
kill "${FAKE_CLIENT_PID}" 2>/dev/null || true
wait "${FAKE_CLIENT_PID}" 2>/dev/null || true

if env -u RJSUPPLICANT_TEST_MODE \
  HOME="${HOME_DIR}" \
  RJSUPPLICANT_SYSTEMD_DIR="${TMP_DIR}/override-systemd" \
  "${ROOT_DIR}/scripts/install.sh" --help >/dev/null 2>&1; then
  printf '未开启测试模式时接受了系统路径覆盖\n' >&2
  exit 1
fi

if HOME="${HOME_DIR}" \
  RJSUPPLICANT_SYSTEMD_DIR="/etc/systemd/system" \
  "${ROOT_DIR}/scripts/install.sh" --help >/dev/null 2>&1; then
  printf '测试模式接受了 /tmp 之外的系统路径覆盖\n' >&2
  exit 1
fi

"${ROOT_DIR}/scripts/install.sh" --help >/dev/null
if "${ROOT_DIR}/scripts/install.sh" --unknown >/dev/null 2>&1; then
  printf '未知选项未返回失败状态\n' >&2
  exit 1
fi

printf 'install_uninstall: ok\n'
