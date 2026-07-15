#!/usr/bin/env bash
set -euo pipefail

REPOSITORY_URL="https://github.com/tjz123psh/-GUI.git"
REPOSITORY_SSH_URL="git@github.com:tjz123psh/-GUI.git"
ARCHIVE_URL="https://github.com/tjz123psh/-GUI/archive/refs/heads/main.tar.gz"
CLIENT_URL="https://etr.gdufs.edu.cn/wlxg/RG_Supplicant_For_Linux_V1.31.zip"
CLIENT_SHA256="d211d9a6efbe5f9dcc27eb78af9515a279b3e44dfc8580e6801b79e9a4f1eea9"
CLIENT_FILENAME="RG_Supplicant_For_Linux_V1.31.zip"
SOURCE_DIR="${RJSUPPLICANT_SOURCE_DIR:-${HOME:-}/.local/src/rjsupplicant-gui}"

log() {
  printf '[rjsupplicant-bootstrap] %s\n' "$*"
}

die() {
  printf '[rjsupplicant-bootstrap] ERROR: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
用法：bootstrap.sh [选项]

无选项        从 GitHub 下载或安全更新源码，然后运行安装脚本
--uninstall   使用已经下载的源码运行卸载
-h, --help    显示此帮助

可用 RJSUPPLICANT_SOURCE_DIR 指定源码目录。
EOF
}

validate_environment() {
  [[ -n "${HOME:-}" ]] || die "HOME 为空，无法确定源码目录。"
  [[ "${SOURCE_DIR}" == /* && "${SOURCE_DIR}" != "/" ]] ||
    die "源码目录必须是非根绝对路径：${SOURCE_DIR}"

  if [[ -n "${RJSUPPLICANT_BOOTSTRAP_ARCHIVE_URL:-}" ]]; then
    [[ "${RJSUPPLICANT_BOOTSTRAP_TEST_MODE:-0}" == "1" ]] ||
      die "归档地址覆盖仅允许隔离回归测试使用。"
    [[ "${RJSUPPLICANT_BOOTSTRAP_ARCHIVE_URL}" == file:///tmp/* ]] ||
      die "测试归档必须位于 /tmp。"
    ARCHIVE_URL="${RJSUPPLICANT_BOOTSTRAP_ARCHIVE_URL}"
  fi

  if [[ -n "${RJSUPPLICANT_BOOTSTRAP_CLIENT_URL:-}" ||
    -n "${RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256:-}" ]]; then
    [[ "${RJSUPPLICANT_BOOTSTRAP_TEST_MODE:-0}" == "1" ]] ||
      die "客户端地址与校验值覆盖仅允许隔离回归测试使用。"
    [[ "${RJSUPPLICANT_BOOTSTRAP_CLIENT_URL:-}" == file:///tmp/* ]] ||
      die "测试客户端归档必须位于 /tmp。"
    [[ "${RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256:-}" =~ ^[0-9a-f]{64}$ ]] ||
      die "测试客户端 SHA-256 无效。"
    CLIENT_URL="${RJSUPPLICANT_BOOTSTRAP_CLIENT_URL}"
    CLIENT_SHA256="${RJSUPPLICANT_BOOTSTRAP_CLIENT_SHA256}"
  fi
}

validate_checkout() {
  [[ -f "${SOURCE_DIR}/Cargo.toml" ]] || die "下载内容缺少 Cargo.toml。"
  [[ -f "${SOURCE_DIR}/scripts/install.sh" ]] || die "下载内容缺少安装脚本。"
}

update_git_checkout() {
  if [[ ! -e "${SOURCE_DIR}" ]]; then
    log "克隆 GitHub 源码到 ${SOURCE_DIR}。"
    mkdir -p "$(dirname -- "${SOURCE_DIR}")"
    git clone --depth 1 --branch main "${REPOSITORY_URL}" "${SOURCE_DIR}"
    return
  fi

  [[ -d "${SOURCE_DIR}/.git" ]] ||
    die "源码目录已存在但不是 Git 仓库，拒绝覆盖：${SOURCE_DIR}"
  [[ -z "$(git -C "${SOURCE_DIR}" status --porcelain)" ]] ||
    die "源码目录有未提交、已暂存或未跟踪文件，拒绝自动更新。"
  [[ "$(git -C "${SOURCE_DIR}" branch --show-current)" == "main" ]] ||
    die "源码仓库当前不在 main 分支，拒绝自动更新。"
  local origin
  origin="$(git -C "${SOURCE_DIR}" remote get-url origin)"
  [[ "${origin}" == "${REPOSITORY_URL}" || "${origin}" == "${REPOSITORY_SSH_URL}" ]] ||
    die "源码仓库 origin 不是项目 GitHub 地址，拒绝自动执行。"

  log "从 GitHub 检查更新。"
  git -C "${SOURCE_DIR}" fetch origin main
  git -C "${SOURCE_DIR}" merge-base --is-ancestor HEAD FETCH_HEAD ||
    die "本地 main 与 GitHub 已分叉，拒绝自动合并。"
  git -C "${SOURCE_DIR}" merge --ff-only FETCH_HEAD
}

download_archive_checkout() {
  [[ ! -e "${SOURCE_DIR}" ]] ||
    die "源码目录已存在；没有 Git 时拒绝覆盖，请先检查：${SOURCE_DIR}"
  command -v curl >/dev/null 2>&1 || die "找不到 curl。"
  command -v tar >/dev/null 2>&1 || die "找不到 tar。"

  local parent temporary archive checkout
  parent="$(dirname -- "${SOURCE_DIR}")"
  mkdir -p "${parent}"
  temporary="$(mktemp -d "${parent}/.rjsupplicant-download.XXXXXX")"
  archive="${temporary}/source.tar.gz"
  checkout="${temporary}/checkout"
  cleanup_archive() {
    rm -rf -- "${temporary}"
  }
  trap cleanup_archive EXIT

  log "从 GitHub 下载 main 分支源码。"
  curl --fail --silent --show-error --location "${ARCHIVE_URL}" --output "${archive}"
  mkdir "${checkout}"
  tar -xzf "${archive}" -C "${checkout}" --strip-components=1
  [[ -f "${checkout}/Cargo.toml" && -f "${checkout}/scripts/install.sh" ]] ||
    die "GitHub 归档内容不完整。"
  mv "${checkout}" "${SOURCE_DIR}"
  cleanup_archive
  trap - EXIT
}

ensure_official_client_zip() {
  command -v curl >/dev/null 2>&1 || die "找不到 curl。"
  command -v sha256sum >/dev/null 2>&1 || die "找不到 sha256sum。"

  if [[ -n "${RJSUPPLICANT_ZIP:-}" ]]; then
    [[ -f "${RJSUPPLICANT_ZIP}" ]] ||
      die "RJSUPPLICANT_ZIP 指向的文件不存在：${RJSUPPLICANT_ZIP}"
    log "使用指定的官方客户端：${RJSUPPLICANT_ZIP}"
    export RJSUPPLICANT_ZIP
    return
  fi

  local destination actual temporary
  destination="${HOME}/Downloads/${CLIENT_FILENAME}"
  if [[ -f "${destination}" ]]; then
    actual="$(sha256sum "${destination}")"
    actual="${actual%% *}"
    [[ "${actual}" == "${CLIENT_SHA256}" ]] ||
      die "现有客户端 ZIP 校验失败，请检查或移走：${destination}"
    log "官方客户端 ZIP 已存在且校验通过：${destination}"
    RJSUPPLICANT_ZIP="${destination}"
    export RJSUPPLICANT_ZIP
    return
  fi

  mkdir -p "$(dirname -- "${destination}")"
  temporary="$(mktemp "${destination}.part.XXXXXX")"
  cleanup_client_download() {
    rm -f -- "${temporary}"
  }
  trap cleanup_client_download EXIT

  log "从广东外语外贸大学官网下载 Linux 客户端。"
  curl --fail --silent --show-error --location "${CLIENT_URL}" --output "${temporary}"
  actual="$(sha256sum "${temporary}")"
  actual="${actual%% *}"
  [[ "${actual}" == "${CLIENT_SHA256}" ]] ||
    die "学校官方客户端 ZIP 的 SHA-256 校验失败。"
  chmod 600 "${temporary}"
  mv "${temporary}" "${destination}"
  trap - EXIT
  log "客户端已下载并校验：${destination}"
  RJSUPPLICANT_ZIP="${destination}"
  export RJSUPPLICANT_ZIP
}

main() {
  case "${1:-}" in
    "")
      [[ "$#" -eq 0 ]] || die "不支持多个参数。"
      ;;
    --uninstall)
      [[ "$#" -eq 1 ]] || die "--uninstall 不接受其他参数。"
      validate_environment
      [[ -f "${SOURCE_DIR}/scripts/install.sh" ]] ||
        die "找不到已下载的安装脚本：${SOURCE_DIR}/scripts/install.sh"
      exec bash "${SOURCE_DIR}/scripts/install.sh" --uninstall
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

  validate_environment
  if command -v git >/dev/null 2>&1 &&
    [[ "${RJSUPPLICANT_BOOTSTRAP_USE_ARCHIVE:-0}" != "1" ]]; then
    update_git_checkout
  else
    download_archive_checkout
  fi
  validate_checkout
  ensure_official_client_zip
  log "运行项目安装脚本。"
  exec bash "${SOURCE_DIR}/scripts/install.sh"
}

main "$@"
