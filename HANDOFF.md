# rjsupplicant-gui 项目交接文档

- 最后更新：2026-07-14
- 当前版本：0.2.0
- 主分支：`main`
- 远端：`git@github.com:tjz123psh/-GUI.git`

## 1. 交接结论

这是面向 Arch Linux 和 niri 桌面环境的 GDUFS 有线锐捷认证原生客户端。当前版本已完成暖色桌面控制台 UI 重设计、niri 多档窗口宽度适配和一轮客户端实现审计。

当前实现具备安装官方客户端、保存非密码配置、连接/断开认证、管理开机认证、读取真实进程与网线状态、查看日志和执行常用诊断操作的完整闭环。项目不实现锐捷认证协议，而是包装学校提供的闭源 Linux 客户端。

本轮代码已经通过格式、单元测试、Clippy、Release 构建、ShellCheck、desktop 文件、SVG XML 和 diff 空白检查。为避免影响当前网络，验证期间没有主动发起真实校园网认证，也没有改动本机现有 systemd 服务。

详细审计记录见 [AUDIT.md](AUDIT.md)，面向使用者的安装说明见 [README.md](README.md)。

## 2. 产品目标与边界

产品目标：

- 为官方锐捷 Linux 程序提供好看、清晰、不会阻塞的桌面 GUI。
- 在 niri 的三分之一、二分之一、三分之二和全宽列布局中都保持可用。
- 让连接状态、网线状态、认证进程和开机认证状态彼此独立，避免误导。
- 重装 Arch Linux 后可从 GitHub 仓库和官方客户端 zip 恢复安装。
- 所有需要 root 的操作都明确经过 polkit 或终端中的 `sudo`。

边界：

- 不自行实现、破解或模拟锐捷协议。
- 不保证支持学校官方客户端以外的其他认证程序。
- 不把“认证进程正在运行”描述为“账号已经认证成功”；最终结果必须看官方日志。
- 不把项目扩展成通用 NetworkManager 前端。
- Arch Linux 是正式目标，其他发行版只提供手动依赖提示。

## 3. 用户明确要求与视觉约束

用户要的是电脑版、niri 版适配，不是拉宽后的手机界面。视觉方向来自以下参考图：

```text
/home/pang/Pictures/Screenshots/ChatGPT Image 2026年7月14日 13_15_36.png
```

当前界面采用暖白画布、橙色强调色、浅色卡片、桌面侧栏和明确的状态层级。侧栏山景已从参考图的无控件区域裁切为仓库资源 `data/sidebar-landscape.png`。

继续修改 UI 时必须遵守：

- 宽屏优先展示完整桌面信息密度，不要改回窄卡片居中式布局。
- 紧凑侧栏使用真正独立的 icon-only 按钮和 GTK 自然居中，不要靠隐藏文字、负边距或 padding 假对齐。
- 图标优先使用语义一致的 symbolic 图标；更换图标时要同时检查 72px 紧凑侧栏和 tooltip。
- 保持强制浅色方案，因为当前配色和插画没有完成深色版本。
- 视觉修改必须至少实测 640、960、1280、1920 四种窗口宽度。
- niri 适配通过 GTK/libadwaita 响应式断点实现，运行时不依赖 niri IPC，因此在其他 Wayland/X11 桌面也可运行。

本机已安装可复用的设计/开发技能：

```text
~/.codex/skills/frontend-design
~/.codex/skills/designing-gnome-ui
~/.codex/skills/developing-gtk-apps
~/.codex/skills/niri-gtk-design
~/.codex/skills/niri-ipc
```

这些技能只辅助设计和截图验证，不是应用运行依赖。

## 4. 技术栈与仓库结构

技术栈：

- Rust 2024 edition
- GTK 4.10+
- libadwaita 1.6+
- systemd system service
- polkit / `pkexec`
- Bash 安装脚本

关键文件：

```text
Cargo.toml                                      Rust 包和 GTK/libadwaita 依赖
src/main.rs                                     应用入口、单实例窗口、浅色方案和 CSS 初始化
src/config.rs                                   XDG/HOME 路径、设置读写、0600 权限和参数校验
src/system.rs                                   官方客户端命令、提权、systemd、状态、日志和诊断
src/ui.rs                                       所有 GTK 页面、交互、后台任务和响应式断点
data/style.css                                  暖色主题和组件样式
data/sidebar-landscape.png                      完整侧栏底部插画
data/io.github.pang.RjSupplicantGui.svg         应用图标
data/io.github.pang.RjSupplicantGui.desktop     桌面入口模板
scripts/install.sh                              依赖、官方客户端、服务、GUI 和桌面资源安装
README.md                                       用户文档
AUDIT.md                                        本轮代码审计结论与限制
HANDOFF.md                                      本交接文档
```

没有 GtkBuilder XML；界面目前全部在 `src/ui.rs` 中构建。该文件较大，后续功能继续增长时可按 dashboard、diagnostics、navigation 和 async actions 拆分，但拆分不应改变现有断点行为。

## 5. 当前 UI 结构

应用包含两个真实页面：

- “连接”：状态总览、连接/断开、四阶段状态、认证信息、连接方式、开机认证、快捷操作和最近日志。
- “诊断”：合并日志、手动刷新和实时 journal 入口。

完整侧栏中的“连接状态、认证设置、运行状态”会定位到连接页的相应区域；“日志查看”进入诊断页；“设置中心”和“关于我们”打开对话框。快捷键 `Ctrl+1` 打开连接页，`Ctrl+2` 打开诊断页。

响应式规则位于 `src/ui.rs::install_breakpoints`：

| 窗口宽度 | 导航 | 内容布局 | niri 使用场景 |
| --- | --- | --- | --- |
| `< 720sp` | 隐藏侧栏，显示底部导航 | 状态区和卡片单栏 | 约 640px 窄列 |
| `720–1099sp` | 72px 独立图标栏 | 单栏 | 约 960px 半宽列 |
| `1100–1399sp` | 72px 独立图标栏 | 三栏 | 约 1280px 宽列 |
| `>= 1400sp` | 240px 完整图文侧栏 | 三栏 | 约 1920px 全宽列 |

默认窗口为 `960 × 760`，最小请求尺寸为 `420 × 520`。主内容通过 `adw::Clamp` 限制最大阅读宽度；不要仅通过 CSS media query 重做断点，因为 GTK CSS 不支持网页式媒体查询，现有布局切换依赖 `adw::Breakpoint` setter。

## 6. 运行数据与路径

用户数据遵循 XDG 路径：

```text
设置文件：${XDG_CONFIG_HOME:-~/.config}/rjsupplicant-gui/settings.conf
GUI 程序：~/.local/bin/rjsupplicant-gui
官方 wrapper：~/.local/bin/rjsupplicant
官方客户端：${XDG_DATA_HOME:-~/.local/share}/rjsupplicant/{x64|x86}/rjsupplicant
官方日志：${XDG_DATA_HOME:-~/.local/share}/rjsupplicant/{x64|x86}/log/run.log
桌面入口：${XDG_DATA_HOME:-~/.local/share}/applications/io.github.pang.RjSupplicantGui.desktop
应用图标：${XDG_DATA_HOME:-~/.local/share}/icons/hicolor/scalable/apps/io.github.pang.RjSupplicantGui.svg
系统服务：/etc/systemd/system/rjsupplicant.service
```

设置文件仅保存账号、网卡、DHCP 和是否让官方客户端保存密码，权限强制为 `0600`。GUI 不保存密码。

## 7. 核心流程

### 连接认证

1. `collect_settings` 读取表单。
2. `config::validate` 校验账号与网卡字符范围。
3. `config::save` 保存非密码设置。
4. 后台任务调用 `system::authenticate`。
5. 经 `pkexec` 运行官方 wrapper：

```text
~/.local/bin/rjsupplicant -a 1 -d <0|1> -n <网卡> -u <账号> -S <0|1> [-p <密码>]
```

密码框为空时不传 `-p`，由官方客户端尝试复用其已保存密码。密码不为空时会短暂出现在特权进程参数中，这是官方程序只提供命令行密码接口造成的上游限制。

### 断开认证

- 若 `rjsupplicant.service` 正在运行，执行 `systemctl stop rjsupplicant.service`，避免服务重启策略把认证重新拉起。
- 否则直接调用 `rjsupplicant -q`。

### 开机认证

启用开机认证时，GUI 按当前账号、网卡、DHCP 和保存密码选项生成 service，写入 `/etc/systemd/system/rjsupplicant.service`，执行 `daemon-reload`，再执行：

```text
systemctl enable --now rjsupplicant.service
```

关闭时执行：

```text
systemctl disable --now rjsupplicant.service
```

官方程序启动后会自行进入后台，因此 service 必须保持：

```ini
Type=forking
GuessMainPID=yes
```

绝对不要改回 `Type=simple`。旧实现因此出现过“service 显示启用但认证立即被 ExecStop 断开”的严重问题。

service 不保存明文密码。开机认证依赖官方客户端先前保存的密码；若用户关闭“交给官方客户端保存密码”，必须明确提示其开机无人值守认证可能无法完成。

### 状态与日志

`system::load_status` 在后台读取：

- wrapper 和架构对应官方二进制是否都存在；
- `/proc/*/comm` 中是否存在 `rjsupplicant`；
- 认证进程运行时长；
- `systemctl is-enabled` 和 `systemctl is-active`；
- 官方 `run.log` 最近 80 行；
- systemd journal 最近 60 行。

选中网卡的物理链路通过 `/sys/class/net/<nic>/carrier` 判断。默认网卡列表排除 loopback、无线和常见虚拟接口，优先保留具有 sysfs `device` 节点的物理以太网卡。

所有可能阻塞的认证、systemctl、状态和日志操作必须继续放在后台线程，GTK 控件更新只能回到主上下文执行。

## 8. 安装、升级与恢复

标准安装：

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/install.sh
```

官方客户端 zip 查找顺序：

```text
1. RJSUPPLICANT_ZIP 指定路径
2. 仓库目录内 RG_Supplicant_For_Linux*.zip 或 rjsupplicant*.zip
3. ~/Downloads 内同名模式
```

如果没有 zip，安装脚本仍会安装 GUI，但会跳过官方客户端和 systemd 服务。zip 到位后重新运行脚本即可。

从 v0.1 或更早版本升级后必须重新运行 `scripts/install.sh`，或在 GUI 中重新启用一次开机认证，以把旧的 `Type=simple` service 迁移为 `Type=forking`，并写入当前账号和网卡。

安装脚本会删除旧桌面入口 `~/.local/share/applications/rjsupplicant.desktop`，防止应用菜单出现两个图标。

## 9. 开发与验证

安装开发依赖：

```bash
sudo pacman -S --needed rust gtk4 libadwaita polkit desktop-file-utils unzip shellcheck libxml2
```

完整验证：

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
bash -n scripts/install.sh
shellcheck scripts/install.sh
desktop-file-validate data/io.github.pang.RjSupplicantGui.desktop
xmllint --noout data/io.github.pang.RjSupplicantGui.svg
git diff --check
```

当前测试共 9 项，覆盖设置参数校验、认证/断开命令构造、systemd service 内容和 systemd 参数转义。

短启动检查：

```bash
timeout 3s target/release/rjsupplicant-gui
```

该命令只能发现启动和 CSS 解析问题，不能替代视觉截图检查。niri 下修改界面后应保存 640、960、1280、1920 四档截图，并重点检查：

- 导航图标是否自然居中；
- 文本、按钮和状态徽标是否溢出；
- 960px 是否确实转为单栏；
- 1280px 是否保留可读的三栏；
- 1920px 完整侧栏和主内容比例是否协调；
- 所有 tooltip、键盘焦点和禁用/忙碌状态是否正常。

## 10. 高风险修改约束

- 不要在源码、脚本或 service 中写死 `/home/pang`，统一使用 `HOME`、XDG 路径或 `src/config.rs`。
- 不要提交官方客户端 zip、解压后的闭源二进制、用户账号、密码、日志或本机 service。
- 不要把配置权限从 `0600` 放宽。
- 不要未经校验直接把账号或网卡拼入 systemd `ExecStart`；保留 `config::validate` 与 `systemd_quote`。
- 不要把 `systemctl is-active` 当作认证成功信号。
- 不要在 GTK 主线程直接调用 `.status()`、`.output()` 或长时间文件/日志读取。
- 不要在连接 service 托管的认证时直接执行 `-q` 而不停止 service。
- 不要移除缺失官方客户端的 banner 和控件禁用逻辑。
- 不要为了对齐紧凑侧栏图标重新引入隐藏 label 或魔法 padding。

## 11. 已知限制

- 官方客户端是闭源旧程序，类似 `sysctl: 写入错误: 错误的文件描述符` 的兼容性错误无法在 GUI 内部根治。
- 官方客户端没有协议级成功回调，GUI 只能可靠判断进程、链路和服务状态，账号是否通过仍需看日志。
- 首次或修改密码时，密码会短暂出现在官方客户端命令行参数中。
- GUI 写 systemd service 需要 `pkexec`；连接等其他提权操作在没有 `pkexec` 时才回退到 kitty、foot、alacritty 或 xterm 中执行 `sudo`。
- 安装脚本尚无卸载参数，也没有发行包或预编译 GitHub Release。
- 当前只有浅色主题。
- 部分侧栏入口是同一连接页的快捷定位，不是六个独立页面。

## 12. 常见排障

官方客户端未安装：

```bash
ls -l ~/.local/bin/rjsupplicant
ls -l ~/.local/share/rjsupplicant/x64/rjsupplicant
RJSUPPLICANT_ZIP=~/Downloads/RG_Supplicant_For_Linux_V1.31.zip scripts/install.sh
```

开机认证失败：

```bash
command -v pkexec
systemctl status polkit
systemctl cat rjsupplicant.service
systemctl status rjsupplicant.service
journalctl -u rjsupplicant.service -n 120 --no-pager
```

认证结果不明确：

```bash
cat ~/.local/share/rjsupplicant/x64/log/run.log
journalctl -u rjsupplicant.service -n 120 --no-pager
pgrep -a rjsupplicant
cat /sys/class/net/<网卡>/carrier
```

应用菜单出现两个入口：

```bash
rm -f ~/.local/share/applications/rjsupplicant.desktop
update-desktop-database ~/.local/share/applications
```

## 13. 后续建议

按优先级排序：

1. 在真实校园有线网环境完成一次连接、断开、错误密码和重启后的端到端验证，并记录脱敏日志。
2. 增加 `scripts/install.sh --uninstall` 和对应文档。
3. 将 `src/ui.rs` 按页面与组件拆分，保持现有视觉和断点不变。
4. 为官方 zip 缺失提供 GUI 内选择文件并安装的流程。
5. 增加 polkit policy，减少重复授权，同时严格限制可执行动作。
6. 建立 GitHub Actions，自动执行 Rust、Shell、desktop 和 SVG 校验。
7. 提供 Arch PKGBUILD 或 GitHub Release 预编译包。
8. 若继续打磨图标，采用统一的自有 symbolic SVG 集，不要混用多种线宽和视觉语言。

## 14. 接手顺序

下一位开发者或 AI 建议按以下顺序恢复上下文：

1. 阅读本文件、`README.md` 和 `AUDIT.md`。
2. 执行 `git status --short --branch`，确认没有覆盖用户未提交改动。
3. 阅读 `src/config.rs` 和 `src/system.rs`，先理解路径、提权和 service 边界。
4. 阅读 `src/ui.rs::install_breakpoints`、`connect_actions`、`refresh_status` 和 `sidebar_navigation`。
5. 运行完整验证命令。
6. 修改 UI 时对照用户参考图，并完成四档宽度截图检查。
7. 涉及真实认证或本机 systemd 服务前，先说明会影响当前网络和系统状态。
8. 提交前检查是否意外加入官方二进制、账号、密码、日志、截图或临时文件。

交接完成的判断标准不是“程序能编译”，而是安装可恢复、GUI 不阻塞、提权结果可靠、service 生命周期正确、状态文案不误导，并且四档 niri 列宽的布局都通过实图检查。
