# rjsupplicant-gui

面向 Arch Linux 的 GDUFS 有线锐捷认证原生 GTK/libadwaita 客户端。

这个仓库的目标是：在 Arch Linux 重装后，通过一条安装脚本完成 GUI、官方客户端 wrapper、桌面入口和 systemd 服务的安装，不需要手动考虑文件放在哪里。

它不重写锐捷协议，而是调用安装脚本生成的官方 Linux 客户端 wrapper：

```text
~/.local/bin/rjsupplicant
```

## 功能

- 一键连接、断开有线认证
- 保存账号、网卡、DHCP 和是否让官方客户端保存密码
- 密码默认留空，复用官方客户端已经保存的密码
- 需要首次保存或修改密码时，再在“本次修改密码”里填写
- 启用/取消 `rjsupplicant.service` 开机自启
- 在窗口内查看最近日志，也可以打开实时日志
- 使用 `pkexec` 弹出系统授权对话框；没有 `pkexec` 时回退到终端 + `sudo`

## 依赖

不需要额外下载前端 UI 库。界面使用系统原生 GTK4 + libadwaita。

Arch Linux:

```bash
sudo pacman -S --needed rust gtk4 libadwaita polkit
```

如果希望没有图形授权工具时还能回退到终端，至少安装一个终端：

```bash
sudo pacman -S --needed kitty
```

## 一键安装

Arch Linux 重装后，执行：

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/install.sh
```

安装脚本会：

- 安装/确认 Rust、GTK4、libadwaita、polkit、desktop-file-utils、unzip
- 构建并安装 `rjsupplicant-gui`
- 安装桌面入口，并删除旧的终端启动器入口
- 从 `RG_Supplicant_For_Linux*.zip` 安装官方客户端
- 生成 `~/.local/bin/rjsupplicant` wrapper
- 生成 `/etc/systemd/system/rjsupplicant.service`

官方客户端 zip 会自动从这些位置查找：

- `RJSUPPLICANT_ZIP=/path/to/RG_Supplicant_For_Linux_V1.31.zip`
- 仓库目录
- `~/Downloads`

如果首次运行时没有 zip，GUI 仍会安装；把 zip 放到 `~/Downloads` 后重新运行 `scripts/install.sh` 即可补装官方客户端。

## 手动构建和安装

```bash
cargo build --release
install -m 755 target/release/rjsupplicant-gui ~/.local/bin/rjsupplicant-gui
install -m 644 data/io.github.pang.RjSupplicantGui.desktop ~/.local/share/applications/
update-desktop-database ~/.local/share/applications
```

## 密码说明

有两个不同的密码概念：

- 校园网密码：只在首次保存或修改时填写；留空不会覆盖已保存密码。
- 管理员授权：官方客户端需要 root 权限抓包/认证，所以连接、断开和管理 systemd 服务时系统会通过 `pkexec` 请求授权。

如果想做到“点击连接完全不输管理员密码”，需要额外配置 polkit 规则或专用 systemd 服务权限。默认不内置免密规则，避免把提权权限放得过宽。
