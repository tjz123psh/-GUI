# rjsupplicant-gui 项目交接文档

- 最后更新：2026-09-02
- 当前版本：0.3.0（实机联调修复轮已完成；2026-09-02 重启失联事故修复轮：service 单元 Type=forking 缺陷修复 + Wi-Fi 射频状态透出，见 3.3 节）
- 项目状态：后端与提权链功能冻结（提权边界与状态可信度三轮修复已解除其中被用户明确要求修改的条目）；前端皮肤完成；**2026-09-01 校园有线网实机联调**与 **2026-09-02 重启事故修复轮**已完成；手动认证实测成功；**提权边界轮 / 并发与资源轮 / 状态可信度与渲染资源轮 + 两轮 CI 修复已完成、已提交推送本机已部署，GitHub Actions 全绿**；等待用户最终手动确认错误密码、polkit 交互与开机自启（含修复后的 Type=simple 单元），以及 README「已知安全边界」两项是否进一步处置
- 最终验收代码基线：`9ff5645`
- 当前代码基线：`36d1dac`（提权边界轮 + 并发与资源轮 + 状态可信度与渲染资源轮 + 两轮 CI 修复，已提交并推送，GitHub Actions 全绿）
- 主分支：`main`，与 `origin/main` 同步（工作树干净）
- 远端：`git@github.com:tjz123psh/-GUI.git`

## 1. 交接结论

这是面向 Arch Linux 和 niri 桌面环境的 GDUFS 有线锐捷认证原生客户端。后端、安装器和提权链沿用已审计的冻结实现；当前工作区已完成 GTK4/libadwaita 前端皮肤重构，视觉方向为日系二次元樱花学园沉浸式单窗口（背景贯穿全窗、透明标题栏、玻璃拟态卡片）。

当前实现具备安装官方客户端、保存非密码配置、连接/断开认证、管理开机认证、读取真实进程与网线状态、查看日志和执行常用诊断操作的完整闭环。项目不实现锐捷认证协议，而是包装学校提供的闭源 Linux 客户端。

2026-07-15 已完成最终非联网验收：格式、24 项 Rust 测试、Clippy、Release 构建、ShellCheck、隔离安装/卸载回归、desktop、SVG/polkit XML 和 diff 空白检查全部通过；连接页、设置页和诊断页也在真实 niri 会话中通过 640、960、1280、1920 四档宽度实图检查。当前没有继续修改代码的发布阻塞项，项目进入功能冻结状态，等待校园有线网实机验证。

2026-08-03 前端皮肤轮完成：删除旧四工作区界面（连接/日志/诊断/设置与暖白侧栏），重建为暮色学园单窗口皮肤（自绘场景层 `src/scene.rs` + 玻璃拟态卡片 `src/ui.rs`），并在真实 niri 会话完成 640/960/1280 三档截图、Gemini 逐屏评审与像素级定量验证；修复了刷新误触发服务变更、开关被行高拉伸、窄列月亮/剪影重叠等缺陷。格式、Rust 测试、Clippy、Release 构建均通过。当前未执行真实认证、polkit 授权或 systemd 服务修改。

2026-08-03 樱花皮肤轮（当日跟进）：场景换为 `data/scene/scene-sakura.png`；标题栏改为沉浸式（根 Overlay：场景铺满全窗口、卡片与透明 headerbar 浮层），修复 headerbar 渐变不生效根因（`background-color` 未置透明导致 libadwaita 底色叠加成纯白条）；引入 Tabler 线性玫瑰粉图标（内嵌 PNG + `gdk::Texture::from_bytes`）、状态四格圆形徽章、日志圆底徽章，并整体提升文字对比度。三档截图经 Gemini 评审 9.0/10，640 窄列逐项验收无缺陷。

2026-08-03 阶段 3 渐进披露浮层轮（当日跟进）：主卡片保持核心认证流程，低频入口收进浮层——header 新增"更多工具"齿轮按钮（`gtk::Popover`，340px 宽白磨砂面板），内含"连接设置"（DHCP 自动获取 IP、保存密码两个开关，读 `config::load()` 初值、变更即 `config::save`）与"诊断与工具"五项（测试连通 ping 223.5.5.5 / 重启开机认证 `RestartService` / 打开客户端目录 / 打开实时日志 terminal / 在线帮助），全部接线 `system.rs` 既有后端入口（原 dead_code 现在点亮）；"最近日志"行改为 activatable，点击弹出完整日志浮层（`gtk::TextView` 等宽深紫、可滚动 220-320px）。新增 Tabler 图标 settings/terminal-2/folder-open/help-circle/bulb，统一玫瑰粉。三档截图经 Gemini 复审通过，无大白条/卡片完整/浮层正常。

2026-08-03 布局重设计轮（当日跟进，用户反馈"布局完全没设计感、全部成列在一个面板、很空洞"后执行）：按"行动与感知分离"重构——左侧玻璃卡片改为**纯操作区**（连接设置表单 + 连接/断开/安装 + 最近日志行），右侧新增**监控面板 side-panel**（运行状态大字 big_conn/big_server + 2×2 状态指标网格 stat_grid + 诊断与工具 5 行 side_diag）；**双栏断点 STANDARD_MAX 从 1080 降到 940**（Niri 半屏 954px 即双栏，此前 954<1080 双栏永不出现、内容竖排窄条+右侧空白）；表单改 `.form-line`/`.form-label` 两列对齐（icon+固定 64px label+输入框）；日志不再常驻右侧（防纵向溢出、左轻右重），由主卡"最近日志"行弹出承载；窄屏显示 `narrow_status` 摘要；纵向压缩（big-state 17pt、padding 收紧、side-panel 行 min-height 34px）；默认窗口 1280×820。**白圈根因两处**：①`popover > contents` 未覆盖 libadwaita 默认 1px border + box-shadow（浮层边缘白圈）；②`window.csd` 默认 1px 白色 outline 与 `window.csd.tiled` 1px color-mix 边框（Niri 平铺触发，窗口四周亮橙带）——统一 `box-shadow:none; border:none; outline:none`（含 backdrop/tiled/maximized/fullscreen）。验证：954×820 双栏/640 单栏/1280 双栏三档截图 Gemini 评审 9.5/10 无缺陷；build/clippy/fmt/test 全绿；本机已重装（哈希 7bc8398d）。

2026-08-03 舞台+控制台重设计轮（当日跟进，用户反馈"放几个卡片就完事了/布局没设计感/很空洞"后执行）：删除全部嵌套玻璃小卡与 2×2 指标网格，改为**舞台 + 控制台**——左侧透明舞台直接透出樱花场景（大状态字胶囊 + 自绘链路图 + 4 状态胶囊 pill：客户端/进程/服务/网卡），右侧 420px 单张玻璃控制台（标题/表单三行/开机认证/连接断开安装/最近日志行/诊断 5 行）；断点：<760 控制台 fill+solid 实底 + 顶部紧凑状态条（圆点+状态字+副行）、760-939 紧凑舞台 shift 0.5、≥940 完整舞台 shift 0.58。`scene::Link` 自绘链路层节点图标（左设备右网关）经用户反馈后替换为 Tabler `device-laptop`/`server`（jsdelivr @tabler/icons 2.47.0 下载，rsvg-convert 64px，描边 #b8507c 与既有 14 枚同语言，随节点半径缩放）。**文字对比度统一招式**：GTK4 CSS 无 text-shadow，舞台大字/区块标题/表单 label/headerbar 标题统一「深紫玻璃胶囊 `alpha(#4A3048, 0.45-0.72)` 托底 + 白字」，玻璃卡底 alpha 提至 0.48/0.56/0.64；popover 由白玻璃改为深紫玻璃（alpha 0.92 + 浅粉描边 + 收敛投影，行标题白字/副标题浅粉）——用户反馈白色浮层"字体和阴影很奇怪"后重做，Esc 正常关闭。验证：三档水平/垂直像素剖面确认舞台场景可见有明暗节奏、控制台为独立 420px 窄带不横贯全窗；Tab 顺序（refresh→more→账号→密码→网卡→自启→连接→断开→安装→日志→诊断 5 行）、键盘输入落位、连接失败反馈（stage-big 变 stat-warn 红 #C63F38）、popover 打开/关闭均实机验证；build/clippy -D warnings/fmt/24 项测试全绿；已部署 ~/.local/bin（哈希 3a0df83c，未提交 diff 延续）。

2026-08-03 系统集成验证轮（用户反馈"右上角设置/刷新按钮放左边"+"实时日志终端没输出"后触发，用户确认安装 polkit agent）：headerbar 的刷新/设置按钮从 `pack_end` 改为 `pack_start`（左上角，标题胶囊跟随在按钮右侧）；「打开实时日志」终端之前为空是正常现象（journalctl 按 unit 过滤，服务从未运行过、无条目）；为触发真实特权链路，用 `gsudo` 跑 helper `enable-service`，**发现并修复 unit 模板 bug**：systemd `WorkingDirectory=` 不接受引号（`ExecStart` 接受），`src/privileged.rs` 的 `service_file()`/`service_content_uses_owned_paths()` 中 workdir 去掉 `systemd_quote`（system.rs 的 GUI 侧模板本就无引号，未动），helper 重部署（745808 字节）后写 unit + enable 成功；**服务启动失败=官方 2014 闭源客户端在 root 下 SEGV**（栈 `CContextControlThread::DispathMessage→CLnxThread::Run→__libc_msgrcv`、`GetDHCPIPInfo`，与现代 glibc 不兼容），普通用户运行则提示权限不够；已 `disable` 服务防开机崩溃循环（unit 文件保留为修复后正确模板），journalctl 留有崩溃栈历史可验证实时日志链路。**polkit agent 补齐**：本机此前无 agent（GUI 特权操作静默失败），`gsudo pacman -S polkit-gnome` 安装，`~/.config/niri/config.kdl` 新增 `spawn-at-startup "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1"`；`pkexec /bin/true` 实测授权弹窗→通过。GUI 部署哈希 2f1b0d19。前端自本轮起不再有新功能改动，等待校园有线网实机验证（第 13 节清单）。前端接线审计补全：`client_requires_migration`/`service_requires_migration`/`service_active` 三个此前未消费的 `ClientStatus` 字段现已接入——控制台顶部新增 `adw::Banner` 迁移横幅（旧版客户端时带"现在处理"按钮直达安装选择器；服务模板不安全时提示操作自启开关），服务状态胶囊区分"已启用/异常/未启用"（enabled 但 `service_active != active` 显示"服务 异常"红点，修复客户端崩溃后误报健康的问题）；安装选择器逻辑提取为 `open_install_dialog()` 供安装按钮与横幅共用。控制台垂直间距压缩（spacing 10→6、行高 34→30、表单 4→2、heading margin 6→2、玻璃卡 padding 24→16）与内容区上边距 48（卡片顶明显低于标题栏关闭按钮）。部署哈希 1bc3d5cd（前端）+ helper 745808 字节。日志区块整合：控制台「最近日志」标题改「日志」，「查看完整日志」行前新增「打开实时日志」行（`system::open_live_log`，与诊断区同一动作），部署哈希 6e45cbf1。

本机已经通过一键脚本安装当前版本。验收时确认 GUI 与 helper 的哈希和当前 Release 完全一致，helper、wrapper、官方客户端及 policy 均为正确的 root-owned 权限，学校官方 ZIP 的 SHA-256 也匹配固定值。验收没有主动发起认证或创建/改动 systemd 服务；测试 GUI 已关闭。

2026-08-04 日志预览区轮（用户"面板显得空"后执行）：控制台「日志」区块新增最近日志预览——等宽字体显示最近 4 行日志（白色玻璃托底），随状态刷新实时更新；内容从 ~570px 填充到 ~750px，底部留白大幅减少。提交 35ff775。

2026-08-04 日志区块精简轮（用户"查看完整日志点击没反应/没有效信息"后执行）：移除「查看完整日志」行与完整日志浮层（TextView+Popover）——预览区已覆盖快速查看，完整日志可用「更多工具」里的「打开实时日志」终端查看；顺带清理 ICON_LOG、.log-text、refresh_status 的 log_text 死代码。提交 5fa0cce。

2026-08-04 网络详情轮（用户选"面板加 IP/网关"）：控制台网卡行下方新增网络详情小字（当前网卡 IPv4 + 默认网关，无地址显示「未获取到 IP」）；后端新增 `system::interface_ipv4`/`interface_gateway`（`ip -4 -o addr show` + `ip route show default` 解析）。提交 aef5739。

2026-08-04 连接详情区轮（用户选"日志下方空白加连接详情"）：控制台日志下方新增「连接详情」——认证账号/连接状态（含时长）/最近认证结果（成功失败+相对时间，会话内 LastAuth 记录，失败在后台闭包记、成功在 run_backend 分支记）；Pango markup 单行分色（标签浅紫灰/值深色、状态粉绿红加粗、值经 markup_escape_text 转义）；默认窗口 860→900。提交 04ebf1d + 336ae9c。

2026-08-04 状态实时刷新轮（用户"能实时显示吗，而不是硬编码"）：run_backend/run_backend_quiet/run_diag 的 set_busy(false) 后无条件 refresh_status（动作后立即反映真实状态；连接失败大状态字会被刷新为「未连接」，toast 保留错误信息）；activate 加 10 秒 `timeout_add_seconds_local` 轮询（busy 时跳过），连接时长/IP/服务状态持续更新。提交 fd0ae5a。

2026-08-04 诊断工具收进浮层轮（用户反馈"为什么设计成点击/手状但点击无效果"+ 选"收进更多工具浮层"后执行）：**根因**——控制台底部「诊断与工具」行此前设置了 pointer cursor 却从未绑定 `connect_activated`（`Ui.diag` 绑定的是浮层内行），用户点击自然无反应；已删除控制台底部整块诊断区。「更多工具」浮层现含连接设置（DHCP/保存密码）+ 诊断与工具 5 行（测试连通/重启开机认证/打开客户端目录/打开实时日志/在线帮助，全部绑定动作）；控制台只留核心流程（账号/密码/网卡/自启/连接断开/安装/查看完整日志），固定无滚动、更简洁。日志区块移除「打开实时日志」行（浮层内有）。控制台间距适度恢复（spacing 2→6、行高 22→26、标题 20pt），内容约 570px 在 860 默认窗口完整显示、底部留白透场景。验证：fmt/clippy/24 测试全绿；浮层键盘打开正常（Tab×2+Enter）；控制台最后内容 y≈568（此前 850）。部署哈希与 target 一致；运行窗口 43。

2026-08-04 交互反馈修复轮（用户反馈"鼠标放右边面板变手状但点击无效果"）：**根因**——`set_busy` 原先只禁用 4 个按钮，诊断行/日志行/自启开关/网卡下拉在忙碌期间保持可点外观（手状光标），但点击被 busy 保护吞掉；而忙碌状态可被 pkexec 弹窗无限期拉长（无超时）。修复：①busy 时禁用全部交互控件（表单/开关/下拉/日志与诊断行），消除"手状但点不动"的误导；②`run_elevated_wait_with_input` 的 pkexec 等待加 120 秒超时（超时 kill 并报错），避免 GUI 永久卡在忙碌态。验证：fmt/clippy/24 测试全绿，部署运行正常。
2026-08-04 固定布局轮（用户反馈"页面可以滚动，要固定住、下面往上拉"+"日志和诊断都有打开实时日志，清除一个"）：**移除重复入口**——诊断与工具区从 5 项减为 4 项（测试连通/重启开机认证/打开客户端目录/在线帮助），「打开实时日志」保留在日志区块；**取消页面滚动**——删除包裹内容的 ScrolledWindow，控制台直接铺在场景层（不再出现滚动条）；内容进一步压缩（console spacing 2、行 min-height 22、标题 19pt、按钮 padding 收紧），默认窗口高 820→860。实测：954 平铺下控制台内容 y90-846 完整显示、无滚动；诊断区 4 行确认。像素验证用 ASCII 图（Gemini 503 不可用）。部署后截图 /tmp/opencode/audit-v2/final-954.png 待用户目验。

2026-08-04 深度审查修复轮（用户要求"深度剖析还存在什么隐患和边界和bug问题"后执行）：审查结论为提权链/ZIP 校验/参数校验扎实，问题集中在 UI 状态一致性与崩溃限流；修复 3 项——①`run_backend` 区分连接类/非连接类成功态（"安装客户端""启用开机认证"此前成功会把舞台误显为"已连接"+成功霞光，现非连接类成功回到平静场景并刷新真实状态）；②开机自启开关失败回滚（enable/disable 失败恢复开关位置并刷新，避免 UI 与服务真实状态脱节）；③unit 模板加 `StartLimitIntervalSec=60`/`StartLimitBurst=3`（防官方客户端 SEGV 时 `Restart=on-failure` 无限重启刷日志，实测新 helper 生成 unit 已含限流、SEGV 场景 3 次后停止重试）。部署：helper（哈希 8e3809d9）+ GUI 均重装一致；验证后服务已恢复 disabled/inactive。其他审查发现（未修，记录备查）：校园网密码经官方客户端 `-p` 参数进入进程 argv（ps 可见，官方二进制限制无法根治，建议 README 提示或可选 hidepid）；pkexec 弹窗无超时（用户不响应时 GUI 保持忙碌，取消弹窗可正常返回）；无单实例锁；helper 缺失时 legacy 模式半支持。

2026-08-04 收尾轮（用户「更新交接文档、清理残余文件、收尾」）：**curl 一键安装本机完整检验通过**——用户跑 `bootstrap.sh` 后逐项核验：官方 ZIP SHA-256=`d211d9a6…` 与脚本内 `CLIENT_SHA256` 一致；helper `ff7d1b6e`、GUI `6e45cbf1` 与本地 `target/release` 构建哈希一致（GitHub 最新源码 `cargo build --release`）；helper/wrapper/客户端均 root:root 755、policy root:root 644；`strings helper` 确认 `WorkingDirectory=` 无引号（unit 修复已进入安装产物）；wrapper 的 getconf LONG_BIT 选 x64/x86 正确；policy 6 动作（install-client/authenticate/disconnect/enable-service/disable-service/restart-service）齐全；desktop 入口与图标就位；`pkexec /bin/true` exit=0；`systemctl is-enabled` 为 not-found 属正常（unit 由 GUI 开自启时生成，安装不创建）。**仓库整理**：删除误入的 `.ruff_cache/`、4 个无引用的 Tabler 图标（activity/server-cog）并按需补 `.gitignore`（提交 ffe7479）。**日志终端行为说明（用户曾困惑）**：`journalctl -u rjsupplicant.service -n 120 -f` 无条件回放最近 120 行历史后进入跟随模式；journal 是系统级日志库，卸载/重装/重开终端均不清空历史，服务无新日志时终端停留在历史尾部——这是预期行为，不是刷新问题。**最终状态**：前端、后端、提权链、安装链全部就绪并入库（main 与 origin/main 同步），唯一待办为校园有线网实机验证（第 13 节清单 5 项，验证通过即 v0.3.0 收版）。本机为 curl bootstrap 完整安装的最新构建环境，验证时可直接使用。

2026-09-02 提权边界修复轮（用户"接手本项目，派出子代理极致搜索问题/隐患/边界"后开工第一批；冻结决策 7/8 中"提权链按既有验收冻结"由用户明确要求修复而解除）：三路 `swarm-explorer` 只读审计 + 主 Agent 逐条取证，产出 5 高危/22 中危清单（完整清单在会话报告，尚未入库）。本批修 4 项：**①`helper` 认证早退漏恢复 NetworkManager**（客户端 8 秒内退出或判定失败时直接 return，跳过 `restore_network_services()`，而客户端此时已停掉 NM → 无线被静默切断且报成功）——重构为单一退出循环 + `NetworkRestorer` RAII 守卫，判定提取为纯函数 `classify_auth`；**②`install.sh` 卸载时 `sudo ~/.local/bin/rjsupplicant -q`**（用户可写路径被以 root 执行，违反 `AUDIT.md:8` 自订原则）——新增 `is_root_owned_executable` 闸门；**③`system.rs` 提权目标无闸门 + 终端 `sudo` 回退 `spawn` 后即报成功**——入口统一 `ensure_root_owned_program`（与 helper 同判据），回退改为等待终端退出并传播状态，并与 pkexec 共用新提取的 `wait_for_child`（`ELEVATION_WAIT_TIMEOUT` 120 秒上界，超时 kill + reap 直接子进程——否则"改成等待"会引入新的不对称：密码提示挂起时 GUI 永久忙碌；两条路径同样不回收 pkexec/终端派生的 root 侧孙进程）；文档承诺的终端回退能力**保留**（`AUDIT.md:100/112`、本文 5.1 与冻结决策未删），仅收紧为 root-owned 程序，legacy 用户级 wrapper 从"可被提升"改为"明确拒绝 + 引导迁移"；**④`command_exists` 绝对路径分支漏判执行位** + `preflight_privileges`（破坏性动作前确认 sudo 可用）+ `BUILD_DIR` 尊重 `CARGO_TARGET_DIR`、测试脚本设 `RJSUPPLICANT_KEEP_BUILD=1`（此前"隔离回归"会对真实仓库 `cargo build --release` + `cargo clean`，清空开发者 `target/`）。验证：32 项 Rust 测试（+5）、`clippy -D warnings`、`fmt --check`、`bash -n`、`shellcheck` ×4、`tests/bootstrap.sh`、`tests/install_uninstall.sh` 全绿；H3 先用独立探针在未修复代码上复现红→修复后绿；`is_root_owned_executable` 双 uid 视角验证（首轮 `$((8#755 & 0o022))` 的 bash 底数错误会让合法 root-owned 也被拒，已改 `8#022`）；回归后 `target/` 1.3G / 279 deps 未被动。另把该脚本的清理 trap 改为 `cleanup` 函数并覆盖 `EXIT HUP INT TERM`（防御性；两次被工具超时 SIGKILL 的运行各留下 393M `/tmp/tmp.*`，已手工清除，普通 TERM 下旧 trap 其实也会跑，故不宣称修复该残留）。**审计过程留痕（影响后续会话对子代理报告的信任校准）**：三份内联摘要出现成批虚构（"polkit 四个 action 都 `allow_any=yes`"、"install.sh 明文回显口令 / `.desktop` `chmod 04755`"、"CI 只跑 `cargo check`"、"工作树 8 文件未提交"），全部被主 Agent `grep`/`sed`/实机 `stat` 证伪；根因是本机 `read_file` 与"path 为单个文件"的 `grep` 会返回错位或假"无匹配"结果（子代理也独立撞到并主动作废受污染查询）。**采用任何子代理结论前必须二次取证**；`backend-system-audit` 首轮把 `unwrap_or(true)` 读成 `false`、`priv-boundary-audit` 首轮结论与其完整重跑互相矛盾，均属同一异常。未修清单见 CHANGELOG 同轮"未修记录备查"。

2026-09-02 并发与资源轮（用户对上一批清单确认后说"交由你来决定"，选择继续第二批）：修 3 项——①`refresh_status_polled` 给 10 秒轮询加在途判断（`load_status` 内所有外部命令均无超时，卡住时线程无限累积），但**只节流定时器路径**，手动刷新与动作后刷新仍强制走 `refresh_status`，避免"一次卡住 → 状态永久冻结"；②`new_log_tail` 推进 `log_offset`（旧实现每 200ms 重读全部新增，且 `size <= offset` 与非法 UTF-8 两条都会让成败判定永久返回 `None` 并退化成超时假成功），现在文件变短会重置偏移、坏字节段跳过；③判到失败（`AuthOutcome::Failed`，此路径必然意味着客户端还活着）时 `kill` + `wait` 回收，因为它已不会建立会话却仍持有 argv 里的明文口令。**超时路径明确不杀**（慢网络上"快成功"与"卡死"不可区分，误杀会打断真实认证），故 H1 的口令驻留与该路径假成功仍在。M12 单实例锁**有意推迟**：需先决定 `ExecStartPost` 的 `restore-network` 是否参与同一把锁，否则开机认证的 NM 恢复会被 GUI 动作阻塞。验证见 CHANGELOG 同轮。

2026-09-03 状态可信度与渲染资源轮（第三批，用户"把问题都解决不要留下 bug…然后推送仓库"）：helper 判定不再把"客户端已退出"当成功（依据项目自己实机确认的前台持会话事实；退出码仍不参与判定）+ 加 `/run/rjsupplicant-helper.lock` 的 `flock` 互斥（`restore-network` 有意不加锁，否则开机认证的 NM 恢复会被界面动作挡住）；`scene.rs` 重写帧调度（去掉把控件克隆进自身回调的自引用环、`is_mapped()` 门控、Idle 30fps 预算、链路 Idle 直接退出帧时钟、`Scene` 与 `Link` 共用同一组纯函数结算动画终点）并修 `draw_mist` 的 `identity_matrix()` 矩阵污染（改 save/restore，此前会抹掉 HiDPI 变换使特效层错位）；`ui.rs` 批量收口状态可信度（失败大字按动作类型、busy 一并禁用横幅、账号/密码回车提交、服务 `activating` 第三态、网卡列表每轮重新探测并 `splice` 重建下拉框、选择器非本地路径明确提示、无 Display 不再 panic、胶囊初值不再先亮健康绿点、开机认证要求「保存密码」否则回弹并说明）；`config.rs` 布尔只认明确字面量 + 原子写 + 空 XDG 视为未设置；架构判定收拢到 `privileged::client_arch_dir()`（指针宽度在 aarch64 上会误指 `x64`），helper/GUI/安装器/`client_install` 四处共用；`service_content_uses_owned_paths` 补未知指令白名单（`User=`/`BindPaths=`/`RootDirectory=`/`StandardOutput=file=`/`OnFailure=` 此前一律判"安全"）；删除无引用死 CSS 并合并重复规则（逐属性等价）；给 `tests/install_uninstall.sh` 三个失败路径用例写清真实覆盖边界；README 新增「已知安全边界」。**过程事故（重要）**：把 `scene.rs` 交给 `swarm-worker` 后，主 Agent 在收到"completed"通知后仍继续编辑同一文件，导致两次互相穿插落盘、`frame_due` 签名冲突与重复常量，最终 `git checkout HEAD -- src/scene.rs` 回退重写；结论是"完成通知不代表写入已落盘"，同文件必须严格单一所有者、拿不到稳定 mtime 前不得接手。像素级截图与三档宽度评审本机不可用（`grim` 的图片输出路径被媒体读取护栏 `PreToolUse` 拒绝，按约定不绕过），CSS 改动改用规则集比对证实等价。仍未解决项与其理由见 CHANGELOG 同轮末条。**推送后发现的既有 CI 故障**：`verify.yml` 最后一步 `git diff --check` 在 Arch 容器里 `git: command not found`（exit 127），`454466f`、`f476d85` 两次运行都因此失败而其余步骤全绿——即 main 在本次推送之前就已经是红的。修法：依赖清单加 `git`、checkout 用 `fetch-depth: 2`、该步改为 `git diff --check HEAD^ HEAD`（无父提交时退回原行为），否则这一步即使能跑也只在干净 checkout 上做空比较。

详细审计记录见 [AUDIT.md](AUDIT.md)，面向使用者的安装说明见 [README.md](README.md)。

## 2. 产品目标与边界

产品目标：

- 为官方锐捷 Linux 程序提供好看、清晰、不会阻塞的桌面 GUI。
- 在 niri 的三分之一、二分之一、三分之二和全宽列布局中都保持可用。
- 让连接状态、网线状态、认证进程和开机认证状态彼此独立，避免误导。
- 重装 Arch Linux 后可从 GitHub 仓库和官方客户端 zip 恢复安装。
- 所有需要 root 的操作都明确经过 polkit 或终端中的 `sudo`；被提升执行的程序一律要求 root-owned 且组/其他不可写（GUI、helper、`install.sh` 三处同一判据）。

边界：

- 不自行实现、破解或模拟锐捷协议。
- 不保证支持学校官方客户端以外的其他认证程序。
- 不把“认证进程正在运行”描述为“账号已经认证成功”；最终结果必须看官方日志。
- 不把项目扩展成通用 NetworkManager 前端。
- Arch Linux 是正式目标，其他发行版只提供手动依赖提示。

## 3. 用户明确要求与视觉约束

用户要的是电脑版、niri 版适配，不是拉宽后的手机界面。2026-08-03 经 grilling 逐问确认的视觉契约（当前唯一权威）：

> **后续轮次更新**：樱花皮肤轮把视觉方向从"暮色"切换为"樱花学园"（背景改用 `data/scene/scene-sakura.png` 插画铺底，配色改樱粉白磨砂，标题栏透明沉浸）；布局重设计轮把"Niri 三档"中的 1280 档扩展为**双栏**（左操作卡 + 右监控面板，断点 940）；舞台+控制台轮确立当前布局（左透明舞台 + 右 420px 玻璃控制台）；系统集成轮补齐 polkit agent 并修复 unit 模板（详见第 3.1 节轮次记录）。以下"暮色/Niri 三档"条目保留作为契约演进记录，当前实际以第 3.1 节轮次记录与第 5 节 UI 结构为准。

- **皮肤层**：只改视觉层，后端骨架与交互链路不动。
- **基调**：学园昼夜·轻小说——优雅、二次元、灵动流畅，不用 emoji 等廉价元素。
- **暮色**：深靛蓝 → 落日暖橙的垂直渐变（非纯黑），认证成功时整幅画面向暖橙霞光漫开（签名时刻），失败用冷青闪击 + 短促回落（惊险感），日常星点呼吸、雾层漂移。
- **混合背景**：代码渐变基底 + 可商用的原创具象元素（学园建筑剪影、月亮、薄雾）+ 暮光滤镜统一色相。
- **动效架构**：声明式 CSS 过渡 + 自绘光效层（`src/scene.rs` 逐帧特效），不做全程动画。
- **非对称主从**：宽窗玻璃卡片偏左黄金位、右侧留场景；窄列卡片自动切居中/fill。
- **含蓄角色**：窗边抱膝侧坐的学园制服少女剪影（原创轮廓，侧坐剪影不涉及五官版权），藏在雾中。
- **Niri 三档**：640 纯做事工具（卡片 fill、装饰零负担）；960 场景登场（卡片偏左、剪影从卡片右缘外开始）；1280 展开形态（卡片 440px、场景在右侧黄金分割位）。

实现约定：

- 强制深色（`adw::StyleManager::set_color_scheme(ForceDark)`），当前配色与自绘场景只有深色版本。
- 背景由自绘 `DrawingArea` 承载，不引入外部图片资源；卡片用半透明玻璃拟态（CSS 渐变 + 圆角 + 阴影），不覆盖 libadwaita 具名调色板。
- 断点通过 GTK widget 属性 setter 切换（`connect_resize` 监听场景宽度），不依赖 Niri IPC 或 CSS media query；阈值 760/940。
- 视觉修改必须至少实测 640、960、1280 三种 Niri preset 宽度并截图评审；浮动缩窗时内容可滚动（Overlay 外包裹 ScrolledWindow）。
- 后端、`src/system.rs`、`src/privileged.rs`、helper、安装脚本和 polkit 协议不属于本轮重写范围。

本机已安装可复用的设计/开发技能：

```text
~/.config/opencode/skills/frontend-design/desktop/design/niri-gtk-design
~/.config/opencode/skills/frontend-design/desktop/design/gnome-ui-design
~/.config/opencode/skills/frontend-design/desktop/build/gtk4-libadwaita-app
~/.config/opencode/skills/frontend-design/desktop/validation/desktop-validation
```

这些技能只辅助设计和截图验证，不是应用运行依赖。

## 3.1 前端皮肤重构（2026-08-03）

用户已明确要求**推翻此前所有前端方案**并重新设计。本文件第 3 节描述暖白侧栏界面的旧内容已被 2026-08-03 的暮色学园契约取代，不要当成目标。

### 当前代码状态

2026-08-03 前端皮肤轮是当前工作区唯一前端实现，基于 `7842361` 重建。工作区**不干净且有意保留未提交改动**。改动范围：

- 删除：`src/ui.rs` 旧版与 `src/ui/`（components/layout/navigation/runtime/settings/connection/diagnostics/logs）、`data/style.css`、`data/sidebar-landscape.png`、`data/campus-link-hero.{svg,jpg}`、`data/resources.gresource.xml`、`data/ARTWORK.md`、`data/icons/`、`build.rs`、`DESIGN.md`。
- 新增 `src/scene.rs`：自绘场景层（樱花皮肤加载 `data/scene/scene-sakura.png` 铺底 + 动效层；此前暮色自绘渐变/星点/月亮/剪影/薄雾已替换，Connecting/Success/Failed 三种模式动效保留）。**布局重设计轮新增 `Link` 层**：设备（笔记本图标）↔ 校园网关（服务器图标）圆形节点 + 链路动画，随认证模式点亮（Idle 淡粉 / Connecting 脉冲 / Success 金粉光点流动 + 节点光晕 / Failed 冷红闪烁）。
- 重写 `src/ui.rs`：**舞台 + 控制台**结构（宽屏左侧透明舞台透出场景：大状态字胶囊 + 链路图 + 4 状态胶囊；右侧 420px 单张玻璃控制台：表单 + 动作 + 日志行 + 诊断），断点阈值 760/940，事件接线；`Cargo.toml` 的 gtk4 features 从 `v4_10` 升到 `v4_20`。已删除旧版嵌套玻璃区块（glass-section/stat_grid/side-panel/narrow_status/big_conn/big_server）。
- `src/system.rs` 顶部新增 `#![allow(dead_code)]`：部分后端入口在单窗口皮肤下暂无界面调用者，属后端能力清单，待后续界面接入。

不要执行 `git reset --hard`、自动清理未跟踪文件或恢复旧 stash。后端、`src/system.rs`、`src/privileged.rs`、helper、安装脚本和 polkit 协议没有被本轮功能性重写。

### 上一轮失败的根因（必须避免重犯）

1. **契约倒置**：先自己写"避免清单"（不要渐变按钮、不要装饰、不要卡片），恰好禁掉了参考图本身的风格，然后忠实执行。参考图的风格就是目标，不要用通用"高级感=克制"的教条覆盖用户明确的视觉要求。
2. **平铺背景**：交付物 87.61% 是单一 `#101418`，卡片 `#1d2024` 与背景只差约 13 个灰阶，组件完全浮不起来。暮色皮肤改用完整垂直渐变 + 星点 + 场景剪影，杜绝平铺色。
3. **目录式装配**：界面把 libadwaita 组件目录（boxed-list、PreferencesGroup、ViewSwitcher）按顺序堆起来，读起来就是"硬编码"。当前皮肤把表单收进单张玻璃卡片、让背景场景承担氛围，不再逐目录堆组件。
4. **断点误伤**：963px 宽度触发双栏断点导致内容高度折半、底部大面积空白。断点阈值 760/1080 按 Niri preset 640/960/1280 实测校准；2026-08-03 布局轮把双栏阈值降到 940（Niri 半屏 954 即双栏），并验证每档纵向填充。

### 皮肤轮验收门槛

界面改完后必须给出量化证据，不能只说"构建通过"：

1. `cargo build --locked` 通过，`cargo clippy --locked --all-targets -- -D warnings` 无告警。
2. 在真实 niri 会话中截图（`scripts/capture-widths.sh io.github.pang.RjSupplicantGui <dir> 640 960 1280`）。
3. 640/960/1280 三档逐屏评审：无裁切、无控件畸变（开关保持横向胶囊）、卡片与背景场景无重叠穿帮、窄列装饰零负担。
4. 浮动缩窗验证：内容可滚动，主操作（连接/断开/安装）在任何缩小高度下仍可达。
5. 不得回归 polkit / systemd / root helper 路径；`tests/` 两个隔离回归脚本仍需通过。

### 前端皮肤轮的硬约束

- **必须继续使用 GTK4 + libadwaita**。已评估并否决 Tauri/Electron：本项目全部价值在原生集成（polkit/pkexec 授权、systemd service、`/proc` 进程判定、root helper、约 4000 行经过审计的 Rust），引入 webview 会破坏 pkexec 授权链。GTK4 CSS 支持 `linear-gradient`、`radial-gradient`、`box-shadow`、`border-radius`、`transition`，libadwaita 1.9 还支持 CSS 变量；唯一缺失的是模糊/`backdrop-filter`。
- GTK CSS **没有** `prefers-color-scheme`。当前皮肤强制深色，不实现浅色变体；若未来要支持浅色，需要读 `adw::StyleManager` 并在窗口挂主题类。
- 不要覆盖 libadwaita 的具名调色板颜色，也不要在 `window` 上直接设 `color`——曾因此弄坏主题并把原生对话框染色。
- 响应式用 widget 属性断点（`connect_resize`），不要改用 CSS media query（GTK 不支持）。

## 3.2 实机联调修复轮（2026-09-01，校园有线网实测）

校园有线网环境（eno1 直连、账号 20251003089、`save_password=1`）实测结论与修复，全部有日志/strace/pcap 实证：

1. **点击连接后界面卡死（用户首报）**：`glib::spawn_future` 在主上下文线程执行阻塞体（pkexec 等待循环最长 120 秒），冻结整个 GTK 主循环。修复：`run_backend`/`run_diag`/`run_service_toggle`/`run_backend_quiet`/`refresh_status` 五个入口改为工作线程 + `futures-channel` 异步回传（`mpsc::unbounded` + `StreamExt::next`），UI 更新仍在主线程；新增直接依赖 `futures-channel`/`futures-util`（glib 传递依赖已存在，零新增编译）。空密码不再被 GUI 拦截（复用已保存密码）。
2. **连接后热点 Wi-Fi 断连（用户次报）**：strace 实证官方客户端启动时主动 `systemctl stop NetworkManager`（2014 客户端设计行为）。修复：helper 在认证/断开/自启/重启后自动 `systemctl start NetworkManager`（幂等）；开机认证 unit 加 `ExecStartPost="…helper" restore-network`（新白名单动作，systemd 直接以 root 调用，无需新 polkit 条目；`service_content_uses_owned_paths` 同步校验该行，旧 unit 重新启用即迁移）。实测断开后 NM 保持 active、Wi-Fi 正常。
3. **认证成功但拿不到 IP**：pcap 实证 802.1x 实际成功（EAP 交换推进到服务器授权消息、端口放行、学校 DHCP 正常、48h 租约），但 2014 客户端内置 DHCP 在现代内核上**不发任何报文**（抓包 0 个 DHCP 包），45 秒后报「无法获取动态IP地址」。修复：helper 认证时启动客户端约 8 秒后恢复 NM，NM 内部 DHCP 获取地址并建立 eno1 默认路由，客户端轮询 `/proc/net/route` 确认后即「认证成功」并保持会话（实机：18:07 认证成功、helper 8s 返回、会话保持、eno1=`192.168.129.140/23`）。
4. **helper 认证结果判定重写**：客户端退出码不可靠（失败也返回 0），改按官方日志新增行判定（成功=「认证成功」；失败=`网线没有连接上/无法连接认证服务器/认证失败/无法获取动态IP地址`，立即返回对应错误）；成功后 helper 立即返回（客户端转孤儿进程保持会话），避免 GUI 的 pkexec 120 秒超时杀掉已认证会话；60 秒兜底。
5. **net-tools 依赖**：客户端调用 `ifconfig`（strace 见 7 处），缺失使「正在启用网卡」失败；`scripts/install.sh` 依赖清单加入 net-tools。

已验证链路：GUI→pkexec→helper→wrapper→官方客户端 全程真实跑通；断开路径（helper disconnect → 客户端退出 → NM 保持）。待用户最终手动验证：GUI 内输入密码连接全程 UX、错误密码反馈、polkit 授权对话框交互、开机自启（service 路径，含 ExecStartPost 时序）。

## 3.3 重启失联事故修复轮（2026-09-02，三个子代理并行深度审计）

用户重启后报告：连不上网络、Wi-Fi 连不上、网络设置"不见了"、开机自启疑似有问题。三个只读子代理（unit 缺失 / NM 重启时序 / 网络设置消失）交叉取证结论，全部有实证：

1. **开机自启 unit 从未被创建**（全 journal 无 enable-service/disable-service 的 pkexec 记录，apply 时按需生成）；「自启有问题」的直觉指向真实缺陷：unit 模板 `Type=forking + GuessMainPID=yes` 与官方客户端"前台运行不 daemon 化"的实际行为矛盾——一旦启用必在 30 秒超时失败、ExecStartPost（NM 恢复）永不执行。已改为 **`Type=simple`**（客户端即主进程持会话；ExecStop `-q` 断连；崩溃 Restart 拉起），security 验证器与测试同步。**启用自启的新 unit 用此模板，无需迁移**。
2. **NM 停止→8 秒自动恢复 = helper 既定时序**（09:41:02 stop → 09:41:10 start，实测多次；NM unit `Type=dbus`+`Restart=on-failure`，显式 stop 不触发 systemd 自动重启，且无 D-Bus activation 兜底文件）。设计正确。
3. **「Wi-Fi 连不上 / 网络设置不见」= 跨重启射频持久关闭 + 设置前端按设计隐藏**：用户 09-01 18:27 手动关闭 Wi-Fi → systemd-rfkill 在关机时存档软阻塞（`/var/lib/systemd/rfkill/…:wlan=0`，bluetooth 同为 0）→ 开机恢复 → DMS（Dank Material Shell 设置）在 `wifiEnabled=false` 时隐藏全部 Wi-Fi 网络与已保存列表（`NetworkWifiTab.qml`）。**四个连接档案（iQOO Z10 Turbo/966903-5G/CMCC-966903/有线连接 1）完好**，`NetworkManager.state` 的 `WirelessEnabled=true`。射频已恢复；GUI 副行新增「Wi-Fi 已禁用」提示（`wifi_radio_enabled`）防再次困惑。
4. **「重启后一直连接失败」无日志支持**：重启后唯一一次 09:41 认证即成功（欢迎横幅）；此前失败窗口与射频关闭、NM 8 秒恢复窗口的感知混淆有关。

未发现任何连接档案丢失、任何客户端对 rfkill 的操作、任何维护脚本对 systemd unit 的操作。维护脚本（~/scripts/maintenance）与项目完全无关（0 命中 rjsupplicant）。

## 4. 技术栈与仓库结构

技术栈：

- Rust 2024 edition
- GTK 4.20+（crate feature `v4_20`，本机 GTK 4.22.4）
- libadwaita 1.6+（本机 1.9.2）
- systemd system service
- polkit / `pkexec`
- Bash 安装脚本

关键文件：

```text
Cargo.toml                                      Rust 包和 GTK/libadwaita 依赖（gtk4 v4_20）
src/main.rs                                     应用入口、单实例窗口、深色方案和模块装配
src/lib.rs                                      GUI 与 helper 共用库入口
src/config.rs                                   XDG/HOME 路径、设置读写、0600 权限和参数校验
src/client_install.rs                           ZIP 快照、校验、权限加固、事务安装和 wrapper 生成
src/privileged.rs                               helper 白名单协议、固定路径、认证参数和 root service
src/bin/rjsupplicant-helper.rs                  root helper 入口、客户端调用和 systemd 管理
src/system.rs                                   helper/旧版分流、提权、状态、日志和诊断
src/scene.rs                                    自绘场景层：樱花插画铺底 + 模式动效 + Link 链路层（设备↔网关节点动画）
src/ui.rs:单窗口樱花皮肤：透明舞台（大状态字 + 链路图 + 状态胶囊）+ 420px 玻璃控制台、断点 760/940、事件接线与状态刷新
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

没有 GtkBuilder XML；界面使用 Rust 构建。`src/ui.rs` 装配窗口，`src/scene.rs` 承担背景绘制；继续修改时应保持 GTK 控件只在主上下文更新，并避免把运行状态重新堆回根文件。

## 5. 当前 UI 结构

当前前端是单窗口樱花学园皮肤，**舞台 + 控制台**两层结构（2026-08-03 布局重设计最终形态，取代早前的"左操作卡 + 右监控面板"双栏）：

- **背景层**（`src/scene.rs` 的 `DrawingArea`）：樱花插画 `data/scene/scene-sakura.png` 铺满全窗（按窗口宽度水平裁切：窄列 focus 素材左端 shift=0、中屏 shift=0.5、宽屏 shift=0.58），标题栏透明沉浸（根 Overlay：场景为底，内容 ScrolledWindow 与透明 headerbar 浮层），认证成功时画面向暖粉霞光漫开。
- **舞台层**（`gtk::Box.stage`，透明无卡背景，场景直接透出）：顶部**大状态字**（`stage-big`，26pt 深紫玻璃胶囊托底 + 白字，stat-ok 粉 / stat-warn 暖红）→ 副标题 → **自绘链路图**（`scene::Link`：左侧设备节点（笔记本 Tabler 图标）+ 右侧校园网关节点（服务器图标）圆形玻璃底、中间链路按认证模式点亮——Idle 淡粉 / Connecting 粉脉冲+光点 / Success 亮金粉+光点流动+节点光晕（签名视觉时刻）/ Failed 冷红闪烁；节点图标为 `data/icons/device-laptop.png`、`server.png`，随节点半径缩放）→ 底部 4 枚**状态胶囊** `pill`（客户端/进程/服务/网卡，圆点 dot-ok 粉 / dot-warn 红）。
- **控制台层**（`gtk::Box.glass-card.console`，420px，唯一玻璃容器）：标题/副标题 → 连接设置表单（`.form-line` 两列对齐：icon + 固定 64px label + 输入框；账号/密码/网卡三行，开机认证 ActionRow+Switch）→ 连接/断开按钮 → 安装按钮 → 最近日志行（activatable 弹完整日志浮层）→ 诊断与工具 5 行（与"更多工具"浮层共用 `make_diag_rows`）。区块文字标题（`console-heading`）与表单 label 均为深紫玻璃胶囊托底白字。
- **窄屏状态条**（`compact_status`，<760 显示）：圆点 + 状态字 + 副行，替代被隐藏的舞台。

导航形态按窗口宽度切换（`connect_resize` 断点，阈值 760/940）：

| 窗口宽度 | 布局 | 场景 shift | niri 使用场景 |
| --- | --- | --- | --- |
| `< 760` | 控制台单列 fill 铺满（`.glass-card.solid` 实底变体保证可读）+ 顶部状态条；舞台隐藏 | 0（素材左端） | 约 640px 窄列 |
| `760–939` | 紧凑舞台 + 420px 控制台靠右（stage 约 284px） | 0.5（居中） | 约 960px 半宽列 |
| `>= 940` | 完整舞台吃满剩余 + 控制台靠右（1280 时 stage 约 760px） | 0.58（宽屏偏右） | 约 1280px 全宽 |

断点通过 widget 属性 setter 切换，不依赖 Niri IPC 或 GTK CSS media query；Overlay 外包 ScrolledWindow，浮动缩窗高度不足时内容可滚动。所有可能阻塞的状态、认证、日志和服务操作继续在后台任务中执行，GTK 控件只在主上下文更新。

**文字对比度统一招式**：GTK4 CSS 无 `text-shadow`，所有浮在亮樱花背景上的文字（舞台大字、区块标题、表单 label、副标题、headerbar 标题、popover 行）统一用「深紫半透明玻璃胶囊底 + 白/浅粉字 + 收敛下投影」，不依赖发光/描边让文字可读。

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

helper 重新解析并校验参数，再从标准输入读取最多 4096 字节的 UTF-8 密码，然后调用固定 root-owned wrapper。密码不会进入 `pkexec` 或 helper 参数；没有 `pkexec` 时，终端 `sudo` 回退以关闭回显的方式重新提示密码，并等待终端命令结束后传播其退出状态（不再"发完即报成功"）。密码框为空时不传 `-p`，由官方客户端尝试复用已保存密码。闭源客户端只提供命令行接口，因此非空密码仍会短暂出现在官方客户端进程参数中。两条提权路径都先过 `ensure_root_owned_program` 闸门：被执行的程序必须 root-owned 且组/其他不可写，因此 legacy 用户级 wrapper 不再被提升到 root，而是明确报错引导通过 `scripts/install.sh` 重装 root-owned 客户端（与控制台迁移横幅同一指向）。

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

标准安装（先审查脚本，不使用 `curl | sh`）：

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/bootstrap.sh
```

无 Git 时先下载为文件并检查，再运行：

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh \
  -o /tmp/rjsupplicant-bootstrap.sh
sed -n '1,240p' /tmp/rjsupplicant-bootstrap.sh
bash /tmp/rjsupplicant-bootstrap.sh
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

### 安装后清理原则（通用，可转述给其他项目）

核心一句话：**删掉"程序运行中自己能再生成的东西"；保留"重启后依赖它恢复现场、或含密钥与程序本体的东西"**。

本项目的安装脚本只在安装成功后自动清理编译中间产物（`target/`，`cargo clean`，`RJSUPPLICANT_KEEP_BUILD=1` 可跳过保留增量缓存），其余全部保留：

- **保留**：源码目录（更新/卸载依赖它）；`~/Downloads` 的官方客户端 ZIP（已固定 SHA-256 校验、重装与恢复依赖它，且属用户下载区）；已安装的 GUI/helper/客户端/desktop/图标/policy（程序本体）；`settings.conf`（用户偏好，0600）；官方客户端 `run.log` 与 systemd 服务（运行状态与认证结果判断依据，服务由 GUI 按需生成）。
- **删除**：`target/` 编译缓存（由 `Cargo.lock` 可完整重建，删除只影响下次构建速度）。

通用判据（遇到不确定的文件先问三句）：① 程序运行中会自动重建吗？→会：可删；② 重启后依赖它恢复现场吗？（游标、进度、登录态）→依赖：保留；③ 含密钥或删了程序跑不起来吗？→是：保留。三问全过才放进自动清理。

可原样转述给别的项目的一段话：

> 安装脚本末尾应自动清理编译缓存、依赖缓存、历史日志和工具残留——这些程序运行时会自动重建，删除不影响功能，只省空间。但必须保留：状态/游标文件（重启恢复现场用）、含密钥的配置文件、程序本体和依赖目录、用户数据。判断标准就一条：凡"运行中能自己再生成的"删掉，"删了无法恢复或程序跑不起来的"一律不碰。

## 9. 开发与验证

安装开发依赖：

```bash
sudo pacman -S --needed rust gtk4 libadwaita polkit desktop-file-utils unzip net-tools shellcheck libxml2
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

当前 Rust 测试共 33 项，覆盖设置参数校验、新旧认证命令构造、密码标准输入校验、helper 参数拒绝、固定 root service、旧 service 路径拒绝、systemd 参数转义、认证轮询判定优先级（早退优先于日志标记、成功不被"认证失败"字样翻盘）、认证日志偏移前进与截断/坏字节恢复、ZIP 路径/源文件/符号链接校验、权限收紧、客户端安装、wrapper 生成和失败回滚、提权目标 root-owned 闸门、`command_exists` 执行位判定与提权有界等待（超时须终止并回收子进程）；另有 2 个隔离 shell 回归脚本，分别覆盖 curl 引导下载/校验/覆盖保护，以及安装卸载、root-owned 产物、不安全 ZIP、回滚、服务清理、拒绝以 root 执行用户可写 wrapper 和配置保留。

GitHub Actions 的 `Verify` 工作流使用 `archlinux:latest` 容器执行同一组检查，避免 Ubuntu 较旧的 libadwaita 版本与正式目标不一致。工作流在 push、pull request 和手动触发时运行，并使用 `Cargo.lock` 的锁定依赖。

### 2026-07-15 后端基线验收记录

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

该命令只能发现启动和 CSS 解析问题，不能替代视觉截图检查。niri 下修改界面后应保存 640、960、1280 三档截图（`scripts/capture-widths.sh io.github.pang.RjSupplicantGui <输出目录> 640 960 1280`），并重点检查：

- 开关、按钮等控件是否保持原生比例（避免 valign 拉伸）；
- 卡片是否与背景场景重叠穿帮（960 档剪影应从卡片右缘外开始）；
- 窄列是否零装饰（640 档不应出现月亮/剪影）；
- 卡片内容、状态文字和 toast 是否有裁切或溢出；
- 浮动缩窗高度不足时内容是否可滚动、主操作是否仍可达。

### 2026-08-02 当前前端轮验收记录

- `cargo fmt --all -- --check`、`cargo test --locked`（24 项）、`cargo clippy --locked --all-targets -- -D warnings`、`cargo build --locked --release`：通过。
- `bash -n`、ShellCheck、desktop-file-validate、SVG/polkit XML、`git diff --check`：通过。
- `tests/bootstrap.sh`、`tests/install_uninstall.sh`：在 `/tmp` 隔离目录通过；未改动真实 `/etc/systemd`、polkit、客户端或用户配置。
- Niri 1920×1080、预设列宽约 640/960/1280/1920：连接、日志、诊断、设置页面均做了只读实图检查；连接主操作无裁切，960px 为 68px rail，完整侧栏为 240px。
- 连接截图量测（先按 alpha bbox 去除 20px CSD 边）：最高频精确色占比为 9.47%–26.35%，远低于旧失败版本 87.61%；以 HSV 饱和度 `S>=0.35` 计，彩色像素约 2.01%–5.81%；主内容有效行覆盖约 85.9%–95.9%。
- 未验证：真实校园账号/密码、错误密码、polkit agent 授权/取消/保留、systemd 重启后的实际认证结果。

### 2026-08-03 暮色皮肤轮验收记录

- `cargo build --locked`、`cargo test --locked`（11+10+3 项）、`cargo clippy --locked --all-targets -- -D warnings`、`cargo fmt`、`cargo build --locked --release`：全部通过。
- 旧四工作区前端已删除（`src/ui.rs` 旧版、`src/ui/`、`data/style.css`、hero/sidebar 美术、`build.rs`、`DESIGN.md`、`ARTWORK.md`、gresource）；后端模块与安装链无功能性改动。
- 真实 niri 会话三档截图（`scripts/capture-widths.sh io.github.pang.RjSupplicantGui /tmp/opencode/dusk-shots3 640 960 1280`）经 Gemini 逐屏评审：三档均无缺陷，960 档卡片内部纯净（剪影从卡片右缘外开始）、1280 完整场景构图、640 全宽单卡零装饰负担。
- 像素级定量验证：640 下月亮位置恢复深空色（detail 门控生效）、底部 toast 白色文字密度从 0.592% 降为 0.000%（刷新不再误触发服务变更）、640×700 内容完整显示、640×600 核心操作可达且状态区进入滚动区。
- 修复清单：refresh `set_active` 误触发 autostart notify 导致意外启停服务（新增 `refreshing` 标志）；开关被 ActionRow 行高纵向拉伸（`set_valign(Align::Center)`）；960 档卡片内透出剪影分界（detail 0.55→0.45）；窄列月亮被卡片切过（月亮加 detail 门控）；浮动缩窗内容溢出（Overlay 外包 ScrolledWindow）；状态 label 限宽（`max_width_chars(26)`）。
- 未验证：真实校园账号/密码、错误密码、polkit agent 授权/取消/保留、systemd 重启后的实际认证结果。

### 2026-08-03 樱花皮肤轮验收记录

- 结构：`AdwToolbarView` 替换为根 `GtkOverlay`（child=scene 铺满全窗，overlay=ScrolledWindow(card) + 透明 headerbar 浮层），`HeaderBar` 经 `set_halign(Fill)/set_valign(Start)` 浮于顶部，窗口控制按钮在浮层下仍正常。
- 图标：`data/icons/` 11 枚 Tabler 线性图标（SVG 源 + 64px PNG），描边统一 `#b8507c`，通过 `gdk::Texture::from_bytes` + `Image::set_paintable` 加载；应用到表单三行前缀、连接/断开/安装按钮、刷新按钮、状态四格徽章、日志徽章。
- 组件精修：状态四格圆形粉渐变徽章（`.stat-badge`）、日志圆底徽章（`.row-badge`）、输入框内阴影、switch 轨道细化、行 hover/active、副标题与占位符对比度整体提升。
- 三档截图（`capture-widths.sh io.github.pang.RjSupplicantGui /tmp/opencode/whitebar-check 1280 960 640`）经 Gemini 评审 9.0/10，640 窄列逐项验收通过（沉浸标题栏、四格无挤压、"eno1 无网线"完整）。
- 已知遗留：GTK CSD 窗口 buffer 固有约 15px 透明外扩（截图可测、桌面不可见性取决于壁纸，`window { box-shadow:none }` 无效，应用层无法去除）。**2026-08-03 布局轮已进一步压制**：`window.csd`/`.tiled` 变体统一 `box-shadow:none; border:none; outline:none` 后，窗口四周亮橙带与白色描边在截图中消除；buffer 本身若有剩余透明外扩仅取决于壁纸亮度，Niri 平铺 + 深色壁纸下不可见。
- 未验证：真实校园账号/密码、错误密码、polkit agent 授权/取消/保留、systemd 重启后的实际认证结果。

### 2026-08-03 阶段 3 渐进披露浮层验收记录

- `cargo build --release`、`cargo clippy --release -- -D warnings`、`cargo fmt --check`、`cargo test --release`（3 项 lib 测试）：全部通过；无任何验证钩子残留（`GAP_TEST_POPOVER` 门已删除）。
- header 顺序：close → refresh → more（齿轮）。更多工具浮层（`gtk::Popover`，width 340）："连接设置"组（DHCP/保存密码开关，`config::load()` 初值，开关变更即 `config::save(&ui.settings())` 持久化，失败 toast）+ "诊断与工具"组五项（测试连通/重启开机认证/打开客户端目录/打开实时日志/在线帮助，`run_diag` 独立消息）。
- 完整日志浮层：`gtk::TextView`（不可编辑、等宽 `.log-text`、`WrapMode::WordChar`）于 ScrolledWindow（min/max height 220/320、width 360），White label "最近日志 点击查看完整日志" activatable，点击后台加载 `load_status().last_log` 后弹出。
- 截图验证：`stage3-shots.sh`（`niri msg action set-window-width/height --id` + `grim`）在空 workspace 隔离后获取 1280/960/640 三档（`/tmp/opencode/stage3-final/`），Gemini 复审：三档均无大白条、齿轮/刷新按钮正常、卡片完整无裁剪、640 属已知极限（四格紧凑但无变形）、配色协调。
- 未验证：真实校园账号/密码、错误密码、polkit agent 授权/取消/保留、systemd 重启后的实际认证结果；渐进披露的开关/诊断在无客户端/无 polkit 环境下的失败路径未实机触发。

### 2026-08-03 布局重设计轮验收记录

- 触发：用户反馈"布局完全没设计感、全部成列在一个面板上、很空洞"，要求学习开源项目（GNOME Connections / ssh-client-manager / adw-network）的布局方式；根因是断点 1080 高于 Niri 半屏 954，双栏永不出现。
- `cargo build --release`、`cargo clippy --release -- -D warnings`、`cargo fmt --check`、`cargo test`：全部通过。
- 真实 niri 会话三档截图（`/tmp/opencode/final-installed-954.png`、`final-installed-small.png`，窗口 id=83）经 Gemini 逐屏评审 9.5/10：954×820 双栏完整无截断、左右协调、5 行诊断全可见；640 窄屏单栏完整，底部状态区显示；1280 双栏完整、右侧场景留白属视觉呼吸非空洞；无白圈/亮橙边框（两处 CSS 根因见 CHANGELOG）。
- 已重装本机：`setsid scripts/install.sh </dev/null > /tmp/install.log 2>&1 &`（install.sh 内部裸 sudo，必须无 TTY 才走 SUDO_ASKPASS fuzzel 密码框）；/home/pang/.local/bin/rjsupplicant-gui 与 target/release 哈希一致 `7bc8398d`。
- 未验证：真实校园认证、polkit 授权、systemd 重启后行为（同前几轮，等待实机验证）。

### 2026-09-02 提权边界修复轮验收记录

- 触发：用户要求"接手本项目，派出子代理极致搜索问题、隐患、边界"；三路 `swarm-explorer` 只读审计（priv-boundary / backend-system / install-pipeline）+ 主 Agent 逐条二次取证，产出 5 高危 / 22 中危 / 约 25 低危清单。本批只修其中 4 项（H3/H4/H5 + M21/M22-②），其余分批。
- 改动范围：`src/bin/rjsupplicant-helper.rs`（`authenticate` 单一退出循环 + `NetworkRestorer` 守卫 + 新增纯函数 `classify_auth`）、`src/system.rs`（`ensure_root_owned_program` 闸门、终端回退改为等待并传播状态、`spawn_terminal`/`run_terminal` 拆分、`command_exists` 补执行位判定）、`scripts/install.sh`（`is_root_owned_executable`、`preflight_privileges`、`BUILD_DIR` 支持 `CARGO_TARGET_DIR`）、`tests/install_uninstall.sh`（新增用户可写 wrapper 拒绝用例 + 构建隔离）、`CHANGELOG.md`/`HANDOFF.md`。
- 行为变化（有意，且与第 10 节既有约束同向）：legacy 用户级 wrapper 不再被提升到 root，改为报错引导重装 root-owned 客户端；文档承诺的"无 pkexec 时终端 `sudo` 回退"能力保留，仅收紧目标程序可信度并回报真实结果。本机当前为 root-owned 完整安装，`privileged_client_ready()` 为真，故本次闸门对现网部署是空操作。
- `cargo fmt --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`（13 + 13 + 6 = 32 项）、`bash -n` ×2、`shellcheck` ×4、`tests/bootstrap.sh`、`tests/install_uninstall.sh`：全部通过。
- 红→绿证据：H3 先用一次性探针在未修复代码上复现（`SUDO_LOG` 出现 `sudo /tmp/h3-repro-work/home/.local/bin/rjsupplicant -q`），修复后同一探针不再产生任何 sudo 调用并输出可操作说明；`is_root_owned_executable` 用真实 uid 与 `unshare -rU` 的 uid=0 两个视角分别验证 0755 放行 / 0775、0757、0644 拒绝（首轮写法 `$((8#755 & 0o022))` 被 bash 判为底数错误，会让合法 root-owned 也走拒绝，已改 `8#022` 并复验）。
- 构建隔离实证：改动前 `target/` 为 1.3G / 279 个 debug deps，跑完 `tests/install_uninstall.sh` 后仍为 1.3G / 279（此前该脚本会 `cargo clean` 清空整树）。
- 未验证（需要真机或额外授权）：H4 修复的网络恢复行为未在真实认证链路上复跑（本机有活跃会话，重启会话需用户在场授权）；无 polkit 环境的终端回退端到端未测（本机 `pkexec` 存在）；`SuConfig.dat` 是否含可恢复凭据仍未确认，属遗留 H2。

### 2026-09-02 并发与资源轮验收记录

- 触发：用户对上批清单回复"交由你来决定"，选择继续第二批（并发与资源）。
- 改动：`src/ui.rs`（新增 `refresh_status_polled`，仅定时器走节流）、`src/bin/rjsupplicant-helper.rs`（`new_log_tail` 改为 `&mut offset` 并推进、处理截断与非法 UTF-8；`await_auth_result` 接收可变偏移、失败路径 `terminate_client`）。
- `cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked`（13 + 13 + 7 = 33 项）、`cargo build --locked --release`、`bash -n`、`shellcheck`：全部通过。
- 已部署本机：`~/.local/bin/rjsupplicant-gui` 与 `/usr/lib/rjsupplicant-gui/rjsupplicant-helper`（gsudo），helper 哈希与构建产物一致（`3abf55ff0f87…`）。
- 实机运行观察：启动后 niri 窗口标题 "锐捷有线认证" 存在；跨 38 秒（约 4 个 10 秒轮询周期）线程数 22→22→19→19（只降不升，无累积），子进程数恒为 0，stdout/stderr 合计 0 字节（无 GLib/CSS 告警、无 panic）；观察窗口已自行关闭，临时输出文件已删除。
- 未验证：真实认证链路上的偏移前进与"失败即终止"（需重新发起认证，会打断本机当前活跃会话）；超时路径的孤儿进程问题按设计保留，未测。

### 2026-09-03 状态可信度与渲染资源轮验收记录

- 触发：用户要求"把问题都解决不要留下 bug，直到完全解决，然后推送仓库"。第三批（状态可信度、渲染资源、提权纵深）。
- 改动文件：`src/ui.rs`、`src/scene.rs`、`src/config.rs`、`src/privileged.rs`、`src/client_install.rs`、`src/bin/rjsupplicant-helper.rs`、`scripts/install.sh`、`tests/install_uninstall.sh`、`README.md`、`CHANGELOG.md`、`HANDOFF.md`。
- 门禁全绿：`cargo fmt --all --check`、`cargo test --locked`（15 + 22 + 7 = 44 项）、`cargo clippy --locked --all-targets -- -D warnings`、`cargo build --locked --release`、`bash -n` ×4、`shellcheck` ×4、`desktop-file-validate`、`xmllint`（policy）、`git diff --check`、`tests/bootstrap.sh`、`tests/install_uninstall.sh`。
- 部署：GUI → `~/.local/bin/rjsupplicant-gui`（用户身份），helper → `/usr/lib/rjsupplicant-gui/rjsupplicant-helper`（`gsudo`），两者与 `target/release` 逐字节一致（`cmp`）。
- 部署后实机测量（`/proc/<pid>/stat` utime+stime，12 秒窗口）：稳定态 CPU **23.7% 单核**；niri 窗口标题"锐捷有线认证"存在；线程 22、子进程 0；stderr **0 字节**（无 GLib/CSS 告警、无 panic）。该 23.7% 是签名动效（花瓣 + 1920x1080 cover 背景 + 多层渐变）在 30fps 预算下的固有代价，改造前未做 A/B 对照（旧产物已被覆盖），因此只报"当前值"，不宣称降幅。
- 未做/不可用：像素截图与 640/960/1280 三档宽度评审（本机媒体读取护栏拒绝 `grim` 写出图片路径，按约定不绕过）；CSS 改动改用"逐属性规则集比对"证实等价（`.log-text` 2→0、`.row-badge` 1→0、`.console row .title/.subtitle` 各 2→1 且合并块属性一致）；真实认证链路（需口令与在场授权，且会打断当前会话）；`hidepid=2` 与客户端目录权限收紧属系统/产品决策，未代用户执行。
- 已知遗留（非疏漏）：官方客户端 argv 携明文口令（闭源接口只有 `-p`）；root 客户端工作目录 0755 导致其自写配置世界可读（收紧到 0750 会让 GUI 读不到 `run.log`，需专用组 + 重登录才两全）；无判定超时路径仍返回成功且不杀客户端；`ForceDark` 与不套 `ScrolledWindow` 为既有冻结决策。

## 10. 高风险修改约束

- 不要在源码、脚本或 service 中写死 `/home/pang`，统一使用 `HOME`、XDG 路径或 `src/config.rs`。
- 不要提交官方客户端 zip、解压后的闭源二进制、用户账号、密码、日志或本机 service。
- 不要把配置权限从 `0600` 放宽。
- 不要给用户可写的 `~/.local/bin/rjsupplicant` 配置 `auth_admin_keep`；保留授权只能执行 root-owned 固定 helper。
- 不要把非 root-owned 或组/其他可写的程序交给 `pkexec` 或终端 `sudo` 提权执行；GUI（`system::ensure_root_owned_program`）、helper（`is_secure_root_executable`）、安装脚本（`is_root_owned_executable`）三处必须保持同一判据。
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
- 官方客户端启动时会主动停止 NetworkManager（strace 实证），helper 已在认证/断开/自启/重启后自动恢复；期间有约 8 秒的无线离线窗口，属客户端设计行为、无法阻止。
- 官方客户端内置 DHCP 在现代内核上不发任何报文（pcap 实证，认证实际成功、端口放行、学校 DHCP 正常），helper 通过认证后约 8 秒恢复 NM、由 NM 内部 DHCP 补位；该注入依赖客户端的 `/proc/net/route` 轮询判定，属兼容性补丁。
- 官方客户端没有协议级成功回调，GUI 只能可靠判断进程、链路和服务状态，账号是否通过仍需看日志。
- GUI 到 helper 的密码使用标准输入且不回显；首次或修改密码时，密码仍会短暂出现在官方闭源客户端命令行参数中。
- root-owned helper 和 policy 必须先通过安装脚本部署；没有 `pkexec` 时，GUI 才回退到 kitty、foot、alacritty 或 xterm 中用 `sudo` 调用同一 helper。
- 旧版用户级客户端回退仍保留用于迁移，但它不具备 root-owned helper 的完整权限边界，应尽快通过重新选择官方 ZIP 完成迁移。
- policy 与 helper 已安装到本机并核对属主、权限和文件内容，但尚未在真实 polkit agent 上触发授权、取消或保留授权流程。
- 项目定位为个人自用，通过 GitHub 保存和同步源码，不维护 Arch/AUR 包或预编译 Release。
- 当前只有深色主题（`ColorScheme::ForceDark`）；自绘樱花场景与玻璃卡片没有浅色变体。
- 背景场景与按钮动效由 `src/scene.rs` 逐帧自绘，悬浮缩小窗口且高度不足时状态区需要滚动查看（GTK overlay-scrolling，滚动条仅在滚动时短暂显示）。

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

后端、提权链和安装脚本不安排新的功能开发。**前端当前轮已完成代码与视觉验收，后续只根据用户反馈或实机验证证据继续迭代**，起点和门槛见第 3.1 节。

后端只有在校园有线网实机验证出现问题时再恢复工作，验证范围为：

1. 正确账号和密码能否完成认证，日志是否能明确判断结果。**2026-09-01 已通过 CLI 直测验证：认证成功 + 中文欢迎横幅 + eno1 获 IP（192.168.129.140/23）；GUI 内最终手动确认待用户执行**。
2. 错误密码时 GUI 状态、提示和日志是否符合实际结果。（未测）
3. 手动断开、重新连接以及 service 正在运行时的断开行为是否正确。**手动断开已 CLI 验证（断开成功、NM 保持 active、Wi-Fi 恢复）**。
4. polkit 授权、取消授权和授权保留期间的六个 helper 动作是否正常。**授权（pkexec 弹窗输密码）已多次实测通过；取消/保留协议未专项验证**。
5. 启用开机自动认证后重启，systemd 是否自动认证；关闭自启后是否彻底停止并禁用服务。（未测；unit 已带 ExecStartPost restore-network，注意验证其 8 秒等待时序与 StartLimitBurst 限流）

若以上项目全部通过，`v0.3.0` 即可视为个人使用的最终版本。若出现问题，应先记录操作步骤、界面状态和脱敏后的 `run.log`/journal，再针对具体故障修改；不要在没有复现证据时继续重构。

## 14. 接手顺序

下一位开发者或 AI 建议按以下顺序恢复上下文：

1. **如果任务是前端修改**：先读第 3 节和第 3.1 节，它们是唯一的前端权威约定（暮色学园契约 + 皮肤轮记录）。然后确认 `git status --short --branch`，保留当前未提交前端改动；旧 UI 文件已删除，`stash@{0}` 仅供查阅失败原因，不要直接恢复。
2. 如果任务是后端问题：先确认用户在第 13 节哪一项实机验证中遇到问题，并收集可复现步骤与脱敏日志。
3. 阅读本文件、`README.md` 和 `AUDIT.md`。
4. 执行 `git status --short --branch`，确认没有覆盖用户未提交改动。
5. 阅读 `src/config.rs` 和 `src/system.rs`，先理解路径、提权和 service 边界。
6. 阅读 `src/ui.rs`（窗口装配、断点、事件）和 `src/scene.rs`（自绘场景层），理解后再改视觉。
7. 运行完整验证命令（`cargo fmt --check`、`cargo test --locked`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo build --locked --release`）。
8. 修改 UI 时对照第 3.1 节的暮色契约与验收门槛，完成 640/960/1280 三档截图和逐屏评审，不要只报告构建通过。
9. 涉及真实认证或本机 systemd 服务前，先说明会影响当前网络和系统状态。
10. 提交前检查是否意外加入官方二进制、账号、密码、日志、截图或临时文件。

交接完成的判断标准不是“程序能编译”，而是安装可恢复、GUI 不阻塞、提权结果可靠、service 生命周期正确、状态文案不误导，并且三档 niri 列宽的布局都通过实图检查。
