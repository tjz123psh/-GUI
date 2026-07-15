# rjsupplicant-gui 项目交接文档

- 最后更新：2026-07-15
- 当前版本：0.3.0
- 项目状态：功能冻结，等待校园有线网实机验证
- 最终验收代码基线：`9ff5645`
- 主分支：`main`
- 远端：`git@github.com:tjz123psh/-GUI.git`

## 1. 交接结论

这是面向 Arch Linux 和 niri 桌面环境的 GDUFS 有线锐捷认证原生客户端。当前版本已完成暖色桌面控制台 UI 重设计、niri 多档窗口宽度适配和一轮客户端实现审计。

当前实现具备安装官方客户端、保存非密码配置、连接/断开认证、管理开机认证、读取真实进程与网线状态、查看日志和执行常用诊断操作的完整闭环。项目不实现锐捷认证协议，而是包装学校提供的闭源 Linux 客户端。

2026-07-15 已完成最终非联网验收：格式、24 项 Rust 测试、Clippy、Release 构建、ShellCheck、隔离安装/卸载回归、desktop、SVG/polkit XML 和 diff 空白检查全部通过；连接页、设置页和诊断页也在真实 niri 会话中通过 640、960、1280、1920 四档宽度实图检查。当前没有继续修改代码的发布阻塞项，项目进入功能冻结状态，等待校园有线网实机验证。

本机已经通过一键脚本安装当前版本。验收时确认 GUI 与 helper 的哈希和当前 Release 完全一致，helper、wrapper、官方客户端及 policy 均为正确的 root-owned 权限，学校官方 ZIP 的 SHA-256 也匹配固定值。验收没有主动发起认证或创建/改动 systemd 服务；测试 GUI 已关闭。

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
src/lib.rs                                      GUI 与 helper 共用库入口
src/config.rs                                   XDG/HOME 路径、设置读写、0600 权限和参数校验
src/client_install.rs                           ZIP 快照、校验、权限加固、事务安装和 wrapper 生成
src/privileged.rs                               helper 白名单协议、固定路径、认证参数和 root service
src/bin/rjsupplicant-helper.rs                  root helper 入口、客户端调用和 systemd 管理
src/system.rs                                   helper/旧版分流、提权、状态、日志和诊断
src/ui.rs                                       窗口、连接页和诊断页装配，共享 UI 状态
src/ui/components.rs                            状态阶段、按钮、标题、日志等通用组件
src/ui/layout.rs                                640/960/1280/1920 响应式断点
src/ui/navigation.rs                            完整/紧凑侧栏及页面定位逻辑
src/ui/runtime.rs                               异步动作、状态刷新、表单收集和控件状态
data/style.css                                  暖色主题和组件样式
data/sidebar-landscape.png                      完整侧栏底部插画
data/io.github.pang.RjSupplicantGui.svg         应用图标
data/io.github.pang.RjSupplicantGui.desktop     桌面入口模板
data/io.github.pang.RjSupplicantGui.policy      按 helper 子命令匹配的 polkit policy
scripts/install.sh                              依赖、官方客户端、服务、GUI 和桌面资源安装/卸载
scripts/bootstrap.sh                            GitHub 源码与学校官方 ZIP 的 curl 引导安装
tests/bootstrap.sh                              隔离下载、校验、覆盖保护和卸载回归
tests/install_uninstall.sh                      隔离 HOME/XDG/systemd/usr 路径的安装卸载回归
.github/workflows/verify.yml                    Arch Linux 自动构建与完整校验
README.md                                       用户文档
AUDIT.md                                        本轮代码审计结论与限制
HANDOFF.md                                      本交接文档
CHANGELOG.md                                    版本变更记录
```

没有 GtkBuilder XML；界面使用 Rust 构建。`src/ui.rs` 保留窗口和两页装配，通用组件、断点、导航、异步动作与状态同步已拆入 `src/ui/` 子模块。继续修改时应保持 GTK 控件只在主上下文更新，并避免把导航或运行状态重新堆回根文件。

## 5. 当前 UI 结构

应用包含两个真实页面：

- “连接”：状态总览、连接/断开、四阶段状态、认证信息、连接方式、开机认证、快捷操作和最近日志。
- “诊断”：合并日志、手动刷新和实时 journal 入口。

完整侧栏中的“连接状态、认证设置、运行状态”会定位到连接页的相应区域；“日志查看”进入诊断页；“关于我们”打开应用信息对话框。“设置中心”是可编辑的原生偏好设置对话框，可修改账号、网卡、DHCP 和是否由官方客户端保存密码；保存后同步更新连接页，关闭对话框则放弃修改。快捷键 `Ctrl+1` 打开连接页，`Ctrl+2` 打开诊断页，`Ctrl+,` 打开设置。

设置对话框已在 niri 下实测 640、960、1280、1920 四档宽度：窄列无裁切，宽列中的表单保持舒适阅读宽度。设置中不提供开机认证开关，避免普通配置修改意外触发管理员授权；该操作仍只在连接页执行。

官方客户端缺失时，顶部 banner 的“选择安装包”会打开原生 `GtkFileDialog`。安装期间 banner 原位显示忙碌状态，连接、自启、重启和客户端目录操作保持禁用；成功后立即刷新真实状态并开放连接操作。

响应式规则位于 `src/ui/layout.rs::install_breakpoints`：

| 窗口宽度 | 导航 | 内容布局 | niri 使用场景 |
| --- | --- | --- | --- |
| `< 720sp` | 隐藏侧栏，显示底部导航 | 状态区和卡片单栏 | 约 640px 窄列 |
| `720–1099sp` | 72px 独立图标栏 | 单栏 | 约 960px 半宽列 |
| `1100–1399sp` | 72px 独立图标栏 | 三栏 | 约 1280px 宽列 |
| `>= 1400sp` | 240px 完整图文侧栏 | 三栏 | 约 1920px 全宽列 |

默认窗口为 `960 × 760`，最小请求尺寸为 `420 × 520`。主内容通过 `adw::Clamp` 限制最大阅读宽度；不要仅通过 CSS media query 重做断点，因为 GTK CSS 不支持网页式媒体查询，现有布局切换依赖 `adw::Breakpoint` setter。

## 6. 运行数据与路径

当前完整安装把所有可被保留 polkit 授权执行的程序放在 root-owned 固定路径：

```text
设置文件：${XDG_CONFIG_HOME:-~/.config}/rjsupplicant-gui/settings.conf
GUI 程序：~/.local/bin/rjsupplicant-gui
特权 helper：/usr/lib/rjsupplicant-gui/rjsupplicant-helper
官方 wrapper：/usr/lib/rjsupplicant-gui/rjsupplicant
官方客户端：/usr/lib/rjsupplicant/{x64|x86}/rjsupplicant
官方日志：/usr/lib/rjsupplicant/{x64|x86}/log/run.log
polkit policy：/usr/share/polkit-1/actions/io.github.pang.RjSupplicantGui.policy
桌面入口：${XDG_DATA_HOME:-~/.local/share}/applications/io.github.pang.RjSupplicantGui.desktop
应用图标：${XDG_DATA_HOME:-~/.local/share}/icons/hicolor/scalable/apps/io.github.pang.RjSupplicantGui.svg
系统服务：/etc/systemd/system/rjsupplicant.service
```

从 v0.2 及更早版本升级时，如果 root-owned 客户端还未安装，GUI 会暂时回退到以下旧路径：

```text
旧 wrapper：~/.local/bin/rjsupplicant
旧客户端：${XDG_DATA_HOME:-~/.local/share}/rjsupplicant/{x64|x86}/rjsupplicant
```

该回退只用于迁移，不匹配项目的保留授权 policy。不要把 `/usr/lib/rjsupplicant-gui/rjsupplicant-helper`、wrapper 或 `/usr/lib/rjsupplicant` 改到用户可写位置。设置文件仅保存账号、网卡、DHCP 和是否让官方客户端保存密码，权限强制为 `0600`；GUI 不保存密码。

## 7. 核心流程

### 在 GUI 中安装官方客户端

1. 缺失 banner 通过 `GtkFileDialog` 选择本机 ZIP。
2. 后台任务调用 `system::install_official_client`，GTK 主线程不执行解压或文件写入。
3. GUI 规范化路径后执行 `pkexec /usr/lib/rjsupplicant-gui/rjsupplicant-helper install-client <绝对路径>`；policy 对安装动作使用 `auth_admin`，不保留本次授权。
4. helper 检查有效 UID 为 0，使用 `O_NOFOLLOW|O_NONBLOCK` 打开普通 ZIP，并从同一文件句柄复制到 `/usr/lib` 下的 `0700` 私有临时目录，避免授权后替换源文件。
5. 固定使用 `/usr/bin/unzip`；解压前拒绝绝对路径、反斜杠逃逸、`..` 以及 ZIP Unix 类型中的链接/特殊文件，解压后再递归拒绝符号链接与特殊文件，将目录收紧为 `0755`、普通文件收紧为 `0644/0755`。
6. 验证当前架构二进制并明确设置可执行权限，生成固定 `/usr/bin/bash`、`/usr/bin/getconf` 且不继承调用方 `LD_LIBRARY_PATH` 的 wrapper。
7. 先暂存旧客户端，再切换新目录和 wrapper；wrapper 安装失败时恢复旧客户端。成功后删除临时目录并刷新真实状态。

完整流程只写 root-owned `/usr/lib/rjsupplicant` 和 `/usr/lib/rjsupplicant-gui/rjsupplicant`，不会创建 systemd 服务。用户之后启用“开机自动认证”时，helper 才按当前设置生成完整 service。

### 连接认证

1. `collect_settings` 读取表单。
2. `config::validate` 校验账号与网卡字符范围。
3. `config::save` 保存非密码设置。
4. 后台任务调用 `system::authenticate`。
5. root-owned 客户端就绪时，经 `pkexec` 运行固定 helper：

```text
/usr/lib/rjsupplicant-gui/rjsupplicant-helper authenticate <DHCP 0|1> <网卡> <账号> <保存 0|1>
```

helper 重新解析并校验参数，再从标准输入读取最多 4096 字节的 UTF-8 密码，然后调用固定 root-owned wrapper。密码不会进入 `pkexec` 或 helper 参数；没有 `pkexec` 时，终端 `sudo` 回退以关闭回显的方式重新提示密码。密码框为空时不传 `-p`，由官方客户端尝试复用已保存密码。闭源客户端只提供命令行接口，因此非空密码仍会短暂出现在官方客户端进程参数中。root-owned 客户端未就绪时才回退到旧用户级 wrapper。

### 断开认证

- 新架构统一调用 helper 的 `disconnect` 白名单动作。
- helper 若检测到 `rjsupplicant.service` 正在运行，使用固定 `/usr/bin/systemctl stop`，避免服务重启策略把认证重新拉起；否则调用固定 root-owned wrapper 的 `-q`。
- 只有 root-owned 客户端未就绪时才使用旧版 systemctl/wrapper 回退。

### 开机认证

启用开机认证时，GUI 把当前账号、网卡、DHCP 和保存密码选项传给 helper。helper 重新校验参数，生成只引用固定 root-owned wrapper 的 service，原子写入 `/etc/systemd/system/rjsupplicant.service`，执行 `daemon-reload`，再执行：

```text
systemctl enable rjsupplicant.service
systemctl restart rjsupplicant.service
```

关闭时通过 helper 执行：

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

polkit policy 使用 `org.freedesktop.policykit.exec.path` 固定 helper，并用 `org.freedesktop.policykit.exec.argv1` 分别匹配六个白名单动作。安装客户端使用 `auth_admin`；连接、断开、启用/关闭/重启服务使用 `auth_admin_keep`。停止、禁用或重启前，helper 会验证 service 是 root-owned、不可写且只引用固定 wrapper；旧 service 必须先通过“启用”动作原子迁移。保留授权永远不能指向 `~/.local/bin` 或其他用户可写程序。

### 状态与日志

`system::load_status` 在后台读取：

- root-owned helper、wrapper 和架构二进制是否就绪，否则检查旧用户级客户端；
- `/proc/*/comm` 中是否存在 `rjsupplicant`；
- 认证进程运行时长；
- `systemctl is-enabled` 和 `systemctl is-active`；
- 官方 `run.log` 最近 80 行；
- systemd journal 最近 60 行。

客户端目录与日志也按相同优先级选择 root-owned 新路径或旧版回退路径。

选中网卡的物理链路通过 `/sys/class/net/<nic>/carrier` 判断。默认网卡列表排除 loopback、无线和常见虚拟接口，优先保留具有 sysfs `device` 节点的物理以太网卡。

所有可能阻塞的认证、systemctl、状态和日志操作必须继续放在后台线程，GTK 控件更新只能回到主上下文执行。

## 8. 安装、升级与恢复

标准安装：

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh | bash
```

bootstrap 默认把源码放到 `~/.local/src/rjsupplicant-gui`。存在 Git 时只允许从项目 origin 对没有修改、暂存或未跟踪文件且未分叉的 `main` 做 fast-forward；没有 Git 时下载 GitHub main 归档，且拒绝覆盖现有目录。随后它从学校官方 `etr.gdufs.edu.cn` 下载 Linux V1.31 ZIP 到 `~/Downloads`，校验固定 SHA-256 `d211d9a6efbe5f9dcc27eb78af9515a279b3e44dfc8580e6801b79e9a4f1eea9` 后通过 `RJSUPPLICANT_ZIP` 交给正式安装脚本。闭源 ZIP 不进入 GitHub 仓库。

重复运行会更新 GUI/helper，但 root-owned 客户端已就绪时默认跳过重装；只有 `RJSUPPLICANT_FORCE_CLIENT_INSTALL=1` 才会再次安装 ZIP。

手动安装仍可使用：

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

安装脚本先构建并安装 root-owned helper 与 polkit policy，再通过 helper 安装 ZIP，最后安装用户级 GUI。如果没有 zip，仍会安装 GUI、helper 和 policy，但会跳过官方客户端；安装阶段始终不创建 systemd 服务。zip 到位后可在 GUI 中点击“选择安装包”，或重新运行脚本。

依赖安装会先执行 `cargo --version` 和 `rustc --version`；现有 rustup 工具链可用时不会安装 pacman 的 `rust` 包，避免两套 cargo 冲突。

从旧版本升级时应重新运行 `scripts/install.sh`，并通过脚本或 GUI 重新选择一次官方 ZIP，把客户端从用户可写路径迁移到 root-owned `/usr/lib`。在迁移完成前，GUI 保留旧路径回退，但该路径不享受项目 policy 的保留授权。客户端迁移后应在 GUI 中重新启用一次开机认证，把旧 service 更新为 `Type=forking`、当前账号/网卡和固定 root-owned wrapper。

安装脚本会删除旧桌面入口 `~/.local/share/applications/rjsupplicant.desktop`，防止应用菜单出现两个图标。

卸载：

```bash
scripts/install.sh --uninstall
```

卸载会中断当前有线认证，停止并删除 `rjsupplicant.service`，必要时通过 helper 或旧 wrapper 断开手动认证进程，再移除 GUI、root-owned helper、polkit policy、新旧 wrapper、官方客户端目录、桌面入口和图标，但保留 `${XDG_CONFIG_HOME:-~/.config}/rjsupplicant-gui` 中的用户偏好。服务停止或手动断开失败时会在删除相关文件前中止。`tests/install_uninstall.sh` 将 HOME、XDG、systemd、libexec、客户端和 policy 路径全部指向临时目录，不接触本机服务或网络。

## 9. 开发与验证

安装开发依赖：

```bash
sudo pacman -S --needed rust gtk4 libadwaita polkit desktop-file-utils unzip shellcheck libxml2
```

完整验证：

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
git diff --check
```

当前 Rust 测试共 24 项，覆盖设置参数校验、新旧认证命令构造、密码标准输入校验、helper 参数拒绝、固定 root service、旧 service 路径拒绝、systemd 参数转义、ZIP 路径/源文件/符号链接校验、权限收紧、客户端安装、wrapper 生成和失败回滚；另有 2 个隔离 shell 回归脚本，分别覆盖 curl 引导下载/校验/覆盖保护，以及安装卸载、root-owned 产物、不安全 ZIP、回滚、服务清理和配置保留。

GitHub Actions 的 `Verify` 工作流使用 `archlinux:latest` 容器执行同一组检查，避免 Ubuntu 较旧的 libadwaita 版本与正式目标不一致。工作流在 push、pull request 和手动触发时运行，并使用 `Cargo.lock` 的锁定依赖。

### 2026-07-15 最终验收记录

- 仓库工作区干净，`main` 与 GitHub 远端一致，验收代码基线为 `9ff5645`。
- 当前安装的 GUI、helper、图标和 policy 与仓库/Release 一致；root-owned 目录和程序均不可由普通用户修改。
- `~/Downloads/RG_Supplicant_For_Linux_V1.31.zip` 的 SHA-256 为 `d211d9a6efbe5f9dcc27eb78af9515a279b3e44dfc8580e6801b79e9a4f1eea9`，与 bootstrap 固定值一致。
- 连接页、设置对话框和诊断页在 niri 的 640、960、1280、1920 宽度下均无控件裁切或主操作缺失。
- 当前没有 `rjsupplicant.service` 属于正常状态；只有用户在 GUI 中启用“开机自动认证”后才会生成服务。
- 当前没有设置文件也属于正常状态；首次保存设置时才会以 `0600` 权限创建。
- 未执行正确密码、错误密码、断开、polkit 授权或重启后的实际认证测试。

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
- 不要给用户可写的 `~/.local/bin/rjsupplicant` 配置 `auth_admin_keep`；保留授权只能执行 root-owned 固定 helper。
- 不要放宽 `HelperRequest` 的子命令或参数数量；新增特权动作必须同时更新解析校验、helper 分支、policy 的 `argv1` 匹配、测试和文档。
- 不要把 helper 中的 `/usr/bin/systemctl`、客户端、wrapper、`/usr/bin/unzip`、wrapper 解释器和 `getconf` 改回基于用户 `PATH` 查找。
- 不要未经校验直接把账号或网卡拼入 systemd `ExecStart`；保留 `config::validate` 与 `systemd_quote`。
- 不要把 `systemctl is-active` 当作认证成功信号。
- 不要在 GTK 主线程直接调用 `.status()`、`.output()` 或长时间文件/日志读取。
- 不要在连接 service 托管的认证时直接执行 `-q` 而不停止 service。
- 不要移除缺失官方客户端的 banner 和控件禁用逻辑。
- 不要为了对齐紧凑侧栏图标重新引入隐藏 label 或魔法 padding。

## 11. 已知限制

- 官方客户端是闭源旧程序，类似 `sysctl: 写入错误: 错误的文件描述符` 的兼容性错误无法在 GUI 内部根治。
- 官方客户端没有协议级成功回调，GUI 只能可靠判断进程、链路和服务状态，账号是否通过仍需看日志。
- GUI 到 helper 的密码使用标准输入且不回显；首次或修改密码时，密码仍会短暂出现在官方闭源客户端命令行参数中。
- root-owned helper 和 policy 必须先通过安装脚本部署；没有 `pkexec` 时，GUI 才回退到 kitty、foot、alacritty 或 xterm 中用 `sudo` 调用同一 helper。
- 旧版用户级客户端回退仍保留用于迁移，但它不具备 root-owned helper 的完整权限边界，应尽快通过重新选择官方 ZIP 完成迁移。
- policy 与 helper 已安装到本机并核对属主、权限和文件内容，但尚未在真实 polkit agent 上触发授权、取消或保留授权流程。
- 项目定位为个人自用，通过 GitHub 保存和同步源码，不维护 Arch/AUR 包或预编译 Release。
- 当前只有浅色主题。
- 部分侧栏入口是同一连接页的快捷定位，不是六个独立页面。

## 12. 常见排障

官方客户端未安装：

```bash
ls -l /usr/lib/rjsupplicant-gui/rjsupplicant-helper
ls -l /usr/lib/rjsupplicant-gui/rjsupplicant
ls -l /usr/lib/rjsupplicant/x64/rjsupplicant
RJSUPPLICANT_ZIP=~/Downloads/RG_Supplicant_For_Linux_V1.31.zip scripts/install.sh
```

开机认证失败：

```bash
command -v pkexec
systemctl status polkit
ls -l /usr/share/polkit-1/actions/io.github.pang.RjSupplicantGui.policy
systemctl cat rjsupplicant.service
systemctl status rjsupplicant.service
journalctl -u rjsupplicant.service -n 120 --no-pager
```

认证结果不明确：

```bash
cat /usr/lib/rjsupplicant/x64/log/run.log
journalctl -u rjsupplicant.service -n 120 --no-pager
pgrep -a rjsupplicant
cat /sys/class/net/<网卡>/carrier
```

应用菜单出现两个入口：

```bash
rm -f ~/.local/share/applications/rjsupplicant.desktop
update-desktop-database ~/.local/share/applications
```

## 13. 后续工作触发条件

当前不安排新的功能开发或界面调整。只有在校园有线网实机验证出现问题时再恢复工作，验证范围为：

1. 正确账号和密码能否完成认证，日志是否能明确判断结果。
2. 错误密码时 GUI 状态、提示和日志是否符合实际结果。
3. 手动断开、重新连接以及 service 正在运行时的断开行为是否正确。
4. polkit 授权、取消授权和授权保留期间的六个 helper 动作是否正常。
5. 启用开机自动认证后重启，systemd 是否自动认证；关闭自启后是否彻底停止并禁用服务。

若以上项目全部通过，`v0.3.0` 即可视为个人使用的最终版本。若出现问题，应先记录操作步骤、界面状态和脱敏后的 `run.log`/journal，再针对具体故障修改；不要在没有复现证据时继续重构。

## 14. 接手顺序

下一位开发者或 AI 建议按以下顺序恢复上下文：

1. 先确认用户在上述哪一项实机验证中遇到问题，并收集可复现步骤与脱敏日志。
2. 阅读本文件、`README.md` 和 `AUDIT.md`。
3. 执行 `git status --short --branch`，确认没有覆盖用户未提交改动。
4. 阅读 `src/config.rs` 和 `src/system.rs`，先理解路径、提权和 service 边界。
5. 阅读 `src/ui/layout.rs::install_breakpoints`、`src/ui/runtime.rs::{connect_actions,refresh_status}` 和 `src/ui/navigation.rs::sidebar_navigation`。
6. 运行完整验证命令。
7. 修改 UI 时对照用户参考图，并完成四档宽度截图检查。
8. 涉及真实认证或本机 systemd 服务前，先说明会影响当前网络和系统状态。
9. 提交前检查是否意外加入官方二进制、账号、密码、日志、截图或临时文件。

交接完成的判断标准不是“程序能编译”，而是安装可恢复、GUI 不阻塞、提权结果可靠、service 生命周期正确、状态文案不误导，并且四档 niri 列宽的布局都通过实图检查。
