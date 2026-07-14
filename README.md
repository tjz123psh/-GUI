# rjsupplicant-gui

面向 Arch Linux 的 GDUFS 有线锐捷认证原生 GTK/libadwaita 客户端。

项目不重写锐捷协议，而是为学校提供的 Linux 官方客户端补上一套可安装、可诊断、可管理开机认证的桌面体验。

## 当前功能

- 连接和断开有线认证，密码可仅使用一次或交给官方客户端保存
- 自动识别物理有线网卡与网线链路状态，不把无线或虚拟接口混入默认列表
- 分开显示官方客户端、认证进程、网线和开机认证状态
- 按当前账号、网卡和 DHCP 设置生成并管理 `rjsupplicant.service`
- 同时显示官方客户端日志和 systemd 日志，并可打开实时日志
- 通过 `pkexec` 请求管理员授权；没有 `pkexec` 时回退到终端 + `sudo`
- 参考暖色桌面控制台设计，提供橙色品牌主题、完整侧栏与独立应用图标
- 针对 niri 的 640/960/1280/1920 列宽分别切换底部导航、图标栏和完整侧栏
- 内置网络连通测试、认证服务重启、客户端目录和学校帮助文档入口
- 授权与日志读取在后台执行，不会冻结 GTK 界面

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

## 一键安装

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/install.sh
```

安装脚本会构建 GUI、安装桌面入口和图标、安装官方客户端 wrapper，并生成 systemd 服务。

官方客户端 zip 按以下顺序查找：

```text
RJSUPPLICANT_ZIP=/path/to/RG_Supplicant_For_Linux_V1.31.zip
仓库目录
~/Downloads
```

首次没有 zip 也可先安装 GUI。之后把 `RG_Supplicant_For_Linux*.zip` 放到 `~/Downloads`，重新运行安装脚本即可。

更新旧版本后，建议重新运行一次安装脚本，或在 GUI 中重新点击“启用”开机认证，以迁移旧的 `Type=simple` 服务文件。

## 密码与权限

- 校园网密码默认不写入 GUI 配置；输入框留空时，复用官方客户端已经保存的密码。
- “交给官方客户端保存密码”关闭时，本次输入的密码只用于当前认证。
- 账号、网卡与开关保存在 `~/.config/rjsupplicant-gui/settings.conf`，权限设置为 `0600`。
- 官方客户端要求 root 权限抓包，因此连接、断开和 systemd 管理会触发系统授权。
- 官方程序只提供命令行密码参数，因此本次密码会短暂出现在特权进程参数中；这是上游客户端接口限制。

## 手动构建与验证

```bash
cargo fmt --check
cargo test
cargo build --release
bash -n scripts/install.sh
shellcheck scripts/install.sh
```

手动安装 GUI：

```bash
install -m 755 target/release/rjsupplicant-gui ~/.local/bin/rjsupplicant-gui
install -m 644 data/io.github.pang.RjSupplicantGui.desktop ~/.local/share/applications/
install -m 644 data/io.github.pang.RjSupplicantGui.svg ~/.local/share/icons/hicolor/scalable/apps/
update-desktop-database ~/.local/share/applications
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```
