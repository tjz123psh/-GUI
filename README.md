# rjsupplicant-gui

面向 Arch Linux 的 GDUFS 有线锐捷认证原生 GTK/libadwaita 客户端。

项目不重写锐捷协议，而是为学校提供的 Linux 官方客户端补上一套可安装、可诊断、可管理开机认证的桌面体验。

## 当前功能

- 连接和断开有线认证，密码可仅使用一次或交给官方客户端保存
- 自动识别物理有线网卡与网线链路状态，不把无线或虚拟接口混入默认列表
- 分开显示官方客户端、认证进程、网线和开机认证状态
- 按当前账号、网卡和 DHCP 设置生成并管理 `rjsupplicant.service`
- 同时显示官方客户端日志和 systemd 日志，并可打开实时日志
- 通过 root-owned helper 和精确匹配子命令的 polkit policy 请求管理员授权；没有 `pkexec` 时回退到终端 + `sudo`
- 参考暖色桌面控制台设计，提供橙色品牌主题、完整侧栏与独立应用图标
- 针对 niri 的 640/960/1280/1920 列宽分别切换底部导航、图标栏和完整侧栏
- 内置网络连通测试、认证服务重启、客户端目录和学校帮助文档入口
- 授权与日志读取在后台执行，不会冻结 GTK 界面
- 安装脚本支持一键卸载，并默认保留权限为 `0600` 的用户设置
- 官方客户端缺失时可在 GUI 中选择学校提供的 ZIP 安装包并立即安装

## v0.3

v0.3 增加 GUI 内官方 ZIP 安装与安全回滚、一键卸载、UI 模块化和 GitHub Actions。官方客户端与特权 helper 现在安装在 root-owned `/usr/lib` 路径，polkit 只允许白名单子命令；安装过程拒绝 ZIP 路径穿越、符号链接和特殊文件，并在替换失败时保留旧客户端。完整版本记录见 [CHANGELOG.md](CHANGELOG.md)。

## v0.2 重要修复

旧版 systemd 单元使用 `Type=simple`，但官方程序启动后会自行进入后台，导致 systemd 把启动器退出误判为服务结束并立刻执行断开。v0.2 改用 `Type=forking`，并对服务参数进行校验和 systemd 转义。

旧版也只读取 `systemctl is-active` 并把它近似当作连接状态。新版直接检查认证进程，并将“开机认证已启用”“认证进程运行中”“网线已插入”作为三个独立状态展示。认证是否最终成功仍以官方客户端日志为准。

完整审计结论见 [AUDIT.md](AUDIT.md)。

## 依赖

Arch Linux：

```bash
sudo pacman -S --needed rust gtk4 libadwaita polkit desktop-file-utils unzip
```

界面使用 GTK 4.10、libadwaita 1.6 或更高版本。若希望在没有图形授权工具时回退到终端，还需要 kitty、foot、alacritty 或 xterm 之一。

运行完整开发验证还需要 `libxml2` 和 `shellcheck`。

## 一键安装

直接从 GitHub 获取引导脚本：

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh | bash
```

该命令会把项目源码下载或安全更新到 `~/.local/src/rjsupplicant-gui`，从广东外语外贸大学官网下载 `RG_Supplicant_For_Linux_V1.31.zip` 到 `~/Downloads`，核对固定 SHA-256 后运行正式安装脚本。已有 Git 仓库只有在 `main` 没有修改、暂存或未跟踪文件，没有分叉且 origin 指向本项目时才会 fast-forward；归档模式不会覆盖已有源码目录。

重复执行会更新源码和应用；root-owned 客户端已就绪时不会重复安装。确实需要重装客户端时，可在命令前设置 `RJSUPPLICANT_FORCE_CLIENT_INSTALL=1`。

不希望直接把网络脚本交给 Bash 时，可先保存和检查：

```bash
curl -fsSLo /tmp/rjsupplicant-bootstrap.sh \
  https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh
less /tmp/rjsupplicant-bootstrap.sh
bash /tmp/rjsupplicant-bootstrap.sh
```

也可以手动克隆后安装：

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/install.sh
```

安装脚本会构建 GUI 与特权 helper，安装桌面入口、图标和 polkit policy，并通过 helper 把官方客户端安装到 root-owned `/usr/lib/rjsupplicant`。安装客户端前会复制一份稳定快照，拒绝 ZIP 中的绝对路径、`..`、符号链接和特殊文件；目录或 wrapper 替换失败时会恢复旧客户端。安装阶段不会创建不含账号和网卡的 systemd 服务，用户在 GUI 中启用开机认证时才会生成完整服务。

官方客户端 zip 按以下顺序查找：

```text
RJSUPPLICANT_ZIP=/path/to/RG_Supplicant_For_Linux_V1.31.zip
仓库目录
~/Downloads
```

首次没有 zip 也可先安装 GUI 与 helper。之后可在应用顶部点击“选择安装包”，直接选择学校提供的 `RG_Supplicant_For_Linux*.zip`；也可以把文件放到 `~/Downloads` 后重新运行安装脚本。

从旧版本升级时，应重新运行安装脚本，再通过安装脚本或 GUI 重新选择一次官方 ZIP，把用户可写的旧客户端迁移到 root-owned 路径。迁移完成前，GUI 仍可回退使用 `~/.local/bin/rjsupplicant`；该回退不会获得本项目 policy 的保留授权。迁移后在 GUI 中重新启用一次开机认证，可把旧服务替换为使用固定 root-owned wrapper 的完整服务。

## 卸载

```bash
~/.local/src/rjsupplicant-gui/scripts/install.sh --uninstall
```

默认源码目录存在时，也可使用对应的 curl 卸载入口：

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh | \
  bash -s -- --uninstall
```

卸载会中断当前有线认证，停止并移除 `rjsupplicant.service`，在需要时断开手动认证进程，然后删除 GUI、root-owned helper、polkit policy、新旧 wrapper、官方客户端目录、桌面入口和图标。脚本默认保留 `${XDG_CONFIG_HOME:-~/.config}/rjsupplicant-gui` 中的账号与网卡偏好；如需彻底清除，可在确认不再需要后手动删除该目录。

## 密码与权限

- 校园网密码默认不写入 GUI 配置；输入框留空时，复用官方客户端已经保存的密码。
- “交给官方客户端保存密码”关闭时，本次输入的密码只用于当前认证。
- 账号、网卡与开关保存在 `~/.config/rjsupplicant-gui/settings.conf`，权限设置为 `0600`。
- 官方客户端要求 root 权限抓包，因此连接、断开和 systemd 管理会触发系统授权。
- helper 固定为 `/usr/lib/rjsupplicant-gui/rjsupplicant-helper`，官方 wrapper 固定为 `/usr/lib/rjsupplicant-gui/rjsupplicant`，客户端位于 `/usr/lib/rjsupplicant`；这些路径必须由 root 拥有，不能改到用户可写目录。
- polkit 通过 helper 的第一个参数区分动作。安装 ZIP 使用 `auth_admin`，每次都重新确认；连接、断开和服务管理使用 `auth_admin_keep`，只对相同白名单动作短期保留授权。
- helper 只接受 `install-client`、`authenticate`、`disconnect`、`enable-service`、`disable-service` 和 `restart-service`，并使用固定的客户端、systemd 与外部工具路径。
- GUI 通过标准输入把校园网密码交给 helper，密码不会出现在 `pkexec` 或 helper 参数中；终端 `sudo` 回退会使用无回显提示重新读取密码。
- 官方程序只提供 `-p` 命令行接口，因此本次密码仍会短暂出现在官方客户端进程参数中；这是上游闭源接口限制。

## 手动构建与验证

```bash
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
bash -n scripts/bootstrap.sh scripts/install.sh
bash -n tests/bootstrap.sh tests/install_uninstall.sh
shellcheck scripts/bootstrap.sh scripts/install.sh
shellcheck tests/bootstrap.sh tests/install_uninstall.sh
tests/bootstrap.sh
tests/install_uninstall.sh
desktop-file-validate data/io.github.pang.RjSupplicantGui.desktop
xmllint --noout data/io.github.pang.RjSupplicantGui.svg
xmllint --noout data/io.github.pang.RjSupplicantGui.policy
```

仅用于开发测试的用户级 GUI 安装如下；完整部署应使用安装脚本，否则不会安装 root-owned helper、客户端和 polkit policy：

```bash
install -m 755 target/release/rjsupplicant-gui ~/.local/bin/rjsupplicant-gui
install -m 644 data/io.github.pang.RjSupplicantGui.desktop ~/.local/share/applications/
install -m 644 data/io.github.pang.RjSupplicantGui.svg ~/.local/share/icons/hicolor/scalable/apps/
update-desktop-database ~/.local/share/applications
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```
