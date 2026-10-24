#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
# 隔离宿主机上可能已有的 /usr/lib/rjsupplicant，避免“已安装”短路掉下载回归。
export RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${TMP_DIR}/host-root-client"
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
  clone)
    clone_destination="${@: -1}"
    if [[ -n "${FAKE_GIT_CLONE_COUNT_FILE:-}" ]]; then
      clone_count=0
      if [[ -f "${FAKE_GIT_CLONE_COUNT_FILE}" ]]; then
        clone_count="$(cat "${FAKE_GIT_CLONE_COUNT_FILE}")"
      fi
      clone_count=$((clone_count + 1))
      printf '%s\n' "${clone_count}" >"${FAKE_GIT_CLONE_COUNT_FILE}"
      if ((clone_count <= ${FAKE_GIT_CLONE_FAILURES:-0})); then
        mkdir -p "${clone_destination}"
        printf 'partial clone\n' >"${clone_destination}/partial"
        exit 1
      fi
    fi
    mkdir -p "${clone_destination}"
    cp -a "${FAKE_GIT_CLONE_SOURCE}/." "${clone_destination}/"
    ;;
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
  fetch)
    if [[ -n "${FAKE_GIT_FETCH_COUNT_FILE:-}" ]]; then
      fetch_count=0
      if [[ -f "${FAKE_GIT_FETCH_COUNT_FILE}" ]]; then
        fetch_count="$(cat "${FAKE_GIT_FETCH_COUNT_FILE}")"
      fi
      fetch_count=$((fetch_count + 1))
      printf '%s\n' "${fetch_count}" >"${FAKE_GIT_FETCH_COUNT_FILE}"
      if ((fetch_count <= ${FAKE_GIT_FETCH_FAILURES:-0})); then
        exit 1
      fi
    fi
    ;;
  merge-base | merge)
    ;;
  *)
    printf 'unexpected fake git invocation: %s\n' "$*" >&2
    exit 1
    ;;
esac
EOF
chmod 755 "${FAKE_BIN}/git"

REAL_CURL="$(command -v curl)"
REAL_STAT="$(command -v stat)"
cat >"${FAKE_BIN}/curl" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "\$*" >>"\${CURL_LOG:-/dev/null}"
exec "${REAL_CURL}" "\$@"
EOF
cat >"${FAKE_BIN}/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
ROOT_CLIENT_FIXTURE="${TMP_DIR}/root-owned-client/x64/rjsupplicant"
mkdir -p "$(dirname -- "${ROOT_CLIENT_FIXTURE}")"
touch "${ROOT_CLIENT_FIXTURE}"
chmod 755 "${ROOT_CLIENT_FIXTURE}"
cat >"${FAKE_BIN}/stat" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ "\${FAKE_ROOT_CLIENT:-0}" == "1" && "\${@: -1}" == "${ROOT_CLIENT_FIXTURE}" ]]; then
  case "\${1:-}:\${2:-}" in
    "-c:%u") printf '0\n'; exit 0 ;;
    "-c:%a") printf '755\n'; exit 0 ;;
  esac
fi
exec "${REAL_STAT}" "\$@"
EOF
chmod 755 "${FAKE_BIN}/curl" "${FAKE_BIN}/sleep" "${FAKE_BIN}/stat"

# clone 失败可能留下半成品目录；重试前必须清理，最多 3 次。
CLONE_DEST="${TMP_DIR}/cloned-source"
CLONE_COUNT="${TMP_DIR}/clone-count"
CLONE_LOG="${TMP_DIR}/clone-retry.log"
HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${CLONE_DEST}" \
RJSUPPLICANT_ZIP="${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
FAKE_GIT_CLONE_COUNT_FILE="${CLONE_COUNT}" \
FAKE_GIT_CLONE_FAILURES=2 \
FAKE_GIT_CLONE_SOURCE="${FIXTURE_ROOT}" \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >"${CLONE_LOG}" 2>&1
[[ "$(cat "${CLONE_COUNT}")" == "3" ]]
grep -Fq '重试' "${CLONE_LOG}"
[[ -f "${CLONE_DEST}/Cargo.toml" ]]
[[ ! -e "${CLONE_DEST}/partial" ]]

HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null

# 显式 RJSUPPLICANT_ZIP 必须走固定 SHA-256 校验；Git fetch 最多重试 3 次。
FETCH_COUNT="${TMP_DIR}/fetch-count"
GIT_RETRY_LOG="${TMP_DIR}/git-retry.log"
HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_ZIP="${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
FAKE_GIT_FETCH_COUNT_FILE="${FETCH_COUNT}" \
FAKE_GIT_FETCH_FAILURES=2 \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >"${GIT_RETRY_LOG}" 2>&1
mapfile -t explicit_result <"${RESULT}"
[[ "${explicit_result[0]}" == "install" ]]
[[ "${explicit_result[1]}" == "${CLIENT_ARCHIVE}" ]]
[[ "$(cat "${FETCH_COUNT}")" == "3" ]]
grep -Fq 'SHA-256 校验通过' "${GIT_RETRY_LOG}"
grep -Fq '重试' "${GIT_RETRY_LOG}"

TAMPERED_ZIP="${TMP_DIR}/tampered-explicit.zip"
printf 'tampered explicit client\n' >"${TAMPERED_ZIP}"
if HOME="${TMP_DIR}/home" \
  PATH="${FAKE_BIN}:${PATH}" \
  RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
  RJSUPPLICANT_ZIP="${TAMPERED_ZIP}" \
  RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
  RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
  BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null 2>&1; then
  printf 'bootstrap 接受了 SHA-256 不匹配的显式 RJSUPPLICANT_ZIP\n' >&2
  exit 1
fi

UNVERIFIED_LOG="${TMP_DIR}/unverified.log"
HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_ZIP="${TAMPERED_ZIP}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
RJSUPPLICANT_ALLOW_UNVERIFIED_ZIP=1 \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >"${UNVERIFIED_LOG}" 2>&1
grep -Fq '风险' "${UNVERIFIED_LOG}"

# 已安装 root-owned 客户端时跳过学校 ZIP 下载；只有显式 FORCE 才会重新下载。
rm -f "${TMP_DIR}/home/Downloads/RG_Supplicant_For_Linux_V1.31.zip"
ROOT_SKIP_LOG="${TMP_DIR}/root-skip.log"
HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${TMP_DIR}/root-owned-client" \
FAKE_ROOT_CLIENT=1 \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >"${ROOT_SKIP_LOG}" 2>&1
mapfile -t skip_result <"${RESULT}"
[[ "${skip_result[0]}" == "install" ]]
[[ -z "${skip_result[1]}" ]]
grep -Fq '跳过学校 ZIP 下载' "${ROOT_SKIP_LOG}"
[[ ! -e "${TMP_DIR}/home/Downloads/RG_Supplicant_For_Linux_V1.31.zip" ]]

HOME="${TMP_DIR}/home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${SOURCE_DIR}" \
RJSUPPLICANT_PRIVILEGED_CLIENT_DIR="${TMP_DIR}/root-owned-client" \
RJSUPPLICANT_FORCE_CLIENT_INSTALL=1 \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
FAKE_ROOT_CLIENT=1 \
BOOTSTRAP_RESULT="${RESULT}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null
mapfile -t force_result <"${RESULT}"
[[ "${force_result[1]}" == "${TMP_DIR}/home/Downloads/RG_Supplicant_For_Linux_V1.31.zip" ]]
[[ -f "${force_result[1]}" ]]

# 所有 curl 调用都必须带统一的重试与超时参数。
CURL_LOG="${TMP_DIR}/curl.log"
CURL_SOURCE_DIR="${TMP_DIR}/curl-source"
HOME="${TMP_DIR}/curl-home" \
PATH="${FAKE_BIN}:${PATH}" \
RJSUPPLICANT_SOURCE_DIR="${CURL_SOURCE_DIR}" \
RJSUPPLICANT_BOOTSTRAP_TEST_MODE=1 \
RJSUPPLICANT_BOOTSTRAP_USE_ARCHIVE=1 \
RJSUPPLICANT_BOOTSTRAP_ARCHIVE_URL="file://${ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_URL="file://${CLIENT_ARCHIVE}" \
RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256="${CLIENT_SHA256}" \
BOOTSTRAP_RESULT="${RESULT}" \
CURL_LOG="${CURL_LOG}" \
  "${ROOT_DIR}/scripts/bootstrap.sh" >/dev/null
[[ "$(wc -l <"${CURL_LOG}")" -eq 2 ]]
while IFS= read -r curl_line; do
  for curl_option in \
    '--retry 3' \
    '--retry-delay 2' \
    '--retry-connrefused' \
    '--connect-timeout 10' \
    '--max-time 300'; do
    [[ "${curl_line}" == *"${curl_option}"* ]] || {
      printf 'curl 缺少参数 %s：%s\n' "${curl_option}" "${curl_line}" >&2
      exit 1
    }
  done
done <"${CURL_LOG}"

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
