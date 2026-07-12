# rjsupplicant-gui 交接说明

这个文件用于后续重新获取上下文。实机演示出问题时，先读这里，再看对应源码。

## 项目定位

面向 Arch Linux 的 GDUFS 有线锐捷认证 GUI。

目标是重装系统后通过仓库和一键脚本恢复：

- GTK4/libadwaita 图形界面
- 官方 Linux 锐捷客户端 wrapper
- 桌面启动入口
- `rjsupplicant.service` systemd 服务
- 开机自启管理

项目不实现锐捷协议，只调用官方客户端。

## 当前仓库

```text
GitHub: git@github.com:tjz123psh/-GUI.git
本地: /home/pang/Projects/rjsupplicant-gui
主分支: main
```

## 关键文件

```text
src/main.rs       应用入口
src/config.rs     路径、配置文件、XDG/HOME 适配
src/system.rs     官方客户端命令、systemd 服务生成、状态读取、日志读取
src/ui.rs         GTK/libadwaita 界面和按钮逻辑
scripts/install.sh 一键安装脚本
data/io.github.pang.RjSupplicantGui.desktop 桌面入口模板
README.md         用户安装说明
HANDOFF.md        本交接文件
```

## 安装方式

重装 Arch Linux 后：

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/install.sh
```

官方客户端 zip 自动查找位置：

```text
RJSUPPLICANT_ZIP=/path/to/RG_Supplicant_For_Linux_V1.31.zip
仓库目录
~/Downloads
```

如果没有 zip，GUI 仍会安装，但官方客户端和 systemd 服务不会完整可用。把 zip 放到 `~/Downloads` 后重新运行安装脚本。

## 当前行为

### 连接网络

GUI 点击“连接网络”：

1. 校验账号不能为空。
2. 保存 GUI 设置到 `~/.config/rjsupplicant-gui/settings.conf`。
3. 调用：

```text
~/.local/bin/rjsupplicant -a 1 -d <0|1> -n <网卡> -u <账号> -S <0|1> [-p <密码>]
```

密码框留空时不会传 `-p`，由官方客户端复用已保存密码。

### 开机自启

GUI 点击“开机自启”：

1. 按当前 GUI 表单生成 `/etc/systemd/system/rjsupplicant.service`。
2. 执行 `systemctl daemon-reload`。
3. 执行 `systemctl enable --now rjsupplicant.service`。

service 的 `ExecStart` 会包含当前账号、网卡、DHCP、保存密码开关，不再依赖安装脚本默认值。

写 systemd 服务使用 `pkexec tee`，所以会弹管理员授权。

### 取消自启

GUI 点击“取消自启”：

```bash
systemctl disable --now rjsupplicant.service
```

同样通过 `pkexec` 授权。

## 验证命令

每次修改后至少跑：

```bash
bash -n scripts/install.sh
shellcheck scripts/install.sh
cargo fmt --check
cargo test
cargo build --release
```

短启动检查 GTK/CSS：

```bash
timeout 3s target/release/rjsupplicant-gui
```

安装当前构建到本机：

```bash
install -m 755 target/release/rjsupplicant-gui ~/.local/bin/rjsupplicant-gui
install -m 644 data/io.github.pang.RjSupplicantGui.desktop ~/.local/share/applications/io.github.pang.RjSupplicantGui.desktop
update-desktop-database ~/.local/share/applications
```

提交推送：

```bash
git status --short
git add .
git commit -m "message"
git push
```

## 常见演示问题排查

### 1. GUI 显示客户端未安装

检查：

```bash
ls -l ~/.local/bin/rjsupplicant
ls -l ~/.local/share/rjsupplicant/x64/rjsupplicant
```

修复：

```bash
cp ~/Downloads/RG_Supplicant_For_Linux_V1.31.zip ~/.local/src/rjsupplicant-gui/
~/.local/src/rjsupplicant-gui/scripts/install.sh
```

### 2. 应用菜单出现两个锐捷图标

旧终端入口应删除：

```bash
rm -f ~/.local/share/applications/rjsupplicant.desktop
update-desktop-database ~/.local/share/applications
```

保留：

```text
~/.local/share/applications/io.github.pang.RjSupplicantGui.desktop
```

### 3. 点击开机自启失败

检查官方客户端是否完整：

```bash
~/.local/bin/rjsupplicant -h
```

检查 polkit/pkexec：

```bash
command -v pkexec
systemctl status polkit
```

检查服务文件：

```bash
systemctl cat rjsupplicant.service
systemctl status rjsupplicant.service
journalctl -u rjsupplicant.service -n 80 --no-pager
```

### 4. 连接后客户端崩溃或提示 sysctl 错误

官方客户端本身较旧，日志里可能出现：

```text
sysctl: 写入错误: 错误的文件描述符
```

这不一定是 GUI 问题。优先看官方客户端日志：

```bash
journalctl -u rjsupplicant.service -n 120 --no-pager
cat ~/.local/share/rjsupplicant/x64/log/run.log
```

### 5. 换网卡后自启仍走旧网卡

在 GUI 里选择新网卡后，重新点一次“开机自启”。这会重写 service。

## 代码注意点

- 不要重新写死 `/home/pang`，路径统一走 `src/config.rs`。
- 不要把官方二进制提交进仓库；安装脚本从 zip 安装。
- `src/system.rs` 里有命令构造测试，改参数时同步测试。
- `pkexec tee` 用于写 `/etc/systemd/system/rjsupplicant.service`，这部分会触发管理员授权。
- UI 使用强制浅色方案，避免系统深色主题把界面染成棕黑色。

## 已知可选增强

- 增加 `scripts/install.sh --uninstall`
- 增加 GUI 内官方 zip 缺失提示和安装引导
- 增加 GitHub release，提供预编译包
- 增加 polkit policy，实现特定命令免重复授权
