#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
FIXTURE_ROOT="${TMP_DIR}/fixture-root"
ARCHIVE="${TMP_DIR}/fixture.tar.gz"
CLIENT_ARCHIVE="${TMP_DIR}/official-client.zip"
SOURCE_DIR="${TMP_DIR}/source"
RESULT="${TMP_DIR}/result"
FAKE_BIN="${TMP_DIR}/bin"

mkdir -p "${FIXTURE_ROOT}/scripts" "${FAKE_BIN}"
printf '[package]\nname = "fixture"\nversion = "0.0.0"\n' >"${FIXTURE_ROOT}/Cargo.toml"
cat >"${FIXTURE_ROOT}/scripts/install.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n%s\n' "${1:-install}" "${RJSUPPLICANT_ZIP:-}" >"${BOOTSTRAP_RESULT}"
EOF
tar -czf "${ARCHIVE}" -C "${TMP_DIR}" "$(basename -- "${FIXTURE_ROOT}")"
printf 'official client fixture\n' >"${CLIENT_ARCHIVE}"
CLIENT_SHA256="$(sha256sum "${CLIENT_ARCHIVE}")"
CLIENT_SHA256="${CLIENT_SHA256%% *}"

HOME="${TMP_DIR}/home" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_USE_ARCHIVE=1 \
RJSUPPLICANT_BOOTSTRAP_ARCHIVE_URL="file://${ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh"

mapfile -t install_result <"${RESULT}"
[[ "${install_result[0]}" == "install" ]]
[[ "${install_result[1]}" == "${TMP_DIR}/home/Downloads/RG_Supplicant_For_Linux_V1.31.zip" ]]
cmp "${CLIENT_ARCHIVE}" "${install_result[1]}"
[[ -f "${SOURCE_DIR}/Cargo.toml" ]]

HOME="${TMP_DIR}/home" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" --uninstall
mapfile -t uninstall_result <"${RESULT}"
[[ "${uninstall_result[0]}" == "--uninstall" ]]

if HOME="${TMP_DIR}/home" \
  RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
  RJSUPPLICANT_BOOTSTRAP_USE_ARCHIVE=1 \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null 2>&1; then
  printf '归档模式覆盖了现有源码目录\n' >&2
  exit 1
fi

mkdir "${SOURCE_DIR}/.git"
cat >"${FAKE_BIN}/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-C" ]]; then
  shift 2
fi
case "${1:-}" in
  status)
    if [[ "${FAKE_GIT_DIRTY:-0}" == "1" ]]; then
      printf '?? .cargo/config\n'
    fi
    ;;
  branch)
    printf 'main\n'
    ;;
  remote)
    printf 'https://github.com/tjz123psh/-GUI.git\n'
    ;;
  fetch | merge-base | merge)
    ;;
  *)
    printf 'unexpected fake git invocation: %s\n' "$*" >&2
    exit 1
    ;;
esac
EOF
chmod 755 "${FAKE_BIN}/git"

HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null

printf 'tampered client\n' >"${TMP_DIR}/home/Downloads/RG_Supplicant_For_Linux_V1.31.zip"
if HOME="${TMP_DIR}/home" \
  PATH="${FAKE_BIN}:${PATH}" \
  RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
  BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null 2>&1; then
  printf 'bootstrap 接受了校验失败的已有客户端 ZIP\n' >&2
  exit 1
fi

if HOME="${TMP_DIR}/home" \
  PATH="${FAKE_BIN}:${PATH}" \
  RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
  FAKE_GIT_DIRTY=1 \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null 2>&1; then
  printf 'bootstrap 接受了带未跟踪配置的 Git 源码目录\n' >&2
  exit 1
fi

if HOME="${TMP_DIR}/home" \
  RJSUPPLICANT_SOURCE_DIR="${TMP_DIR}/unsafe-source" \
  RJSUPPLICANT_BOOTSTRAP_ARCHIVE_URL="file://${ARCHIVE}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null 2>&1; then
  printf '未开启测试模式时接受了归档地址覆盖\n' >&2
  exit 1
fi

"${ROOT_DIR}/scripts/bootstrap.sh" --help >/dev/null
if "${ROOT_DIR}/scripts/bootstrap.sh" --unknown >/dev/null 2>&1; then
  printf '未知 bootstrap 参数未返回失败状态\n' >&2
  exit 1
fi

printf 'bootstrap: ok\n'
