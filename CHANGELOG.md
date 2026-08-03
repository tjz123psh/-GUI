# Changelog

## Unreleased - 2026-08-03

### 安装脚本：构建后自动清理（同日跟进）

- `install.sh` 在安装完成后自动 `cargo clean` 删除源码目录下的编译中间产物（`target/`，约数百 MB 至数 GB），避免源码构建式安装留下构建垃圾；设置 `RJSUPPLICANT_KEEP_BUILD=1` 可跳过清理（保留增量构建缓存），`--help` 已注明。

### 日志区块整合（同日跟进）

- 控制台「最近日志」标题改为「日志」，并在「查看完整日志」行之前新增「打开实时日志」行（点击在终端打开 `journalctl -f`，复用 `system::open_live_log`，与诊断区同一条已验证动作），消除标题与行内容在「日志」概念上的视觉重复。

### 前后端接线补全轮（同日跟进）

- 审计 system.rs 14 个公开接口与 helper 6 个提权动作的 UI 接线（load_status/wired_interfaces/interface_has_carrier/install_official_client/authenticate/disconnect/enable_service/disable_service/restart_service/test_connectivity/open_client_folder/open_help/open_live_log 全部有入口，无死接口）。
- 补齐 3 个后端计算但前端未消费的字段：`client_requires_migration` 与 `service_requires_migration` 改由顶部 `adw::Banner` 呈现（客户端迁移提示带「现在处理」按钮，复用安装对话框；服务迁移提示引导操作自启开关）；`service_active` 接入"服务"状态胶囊三态（已启用=enabled+active / 异常=enabled 未运行 / 未启用），避免官方客户端崩溃时误报服务健康。
- 提取 `open_install_dialog`（文件选择官方 ZIP→helper install-client），安装按钮与 banner 按钮共用入口；修复 enable/restart 错误文案对已删除"顶部提示"区域的引用。

### 控制台垂直压缩与顶部定位（同日跟进，用户反馈"太长了/上边向下点，至少在那个叉号下边"）

- 控制台 7 处压缩：子元素 spacing 10→6、区块标题上边距 6→2、标题胶囊 padding 3→2px、副标题下边距 6→3、表单行内边距 4→2、ActionRow 行高 34→30px。
- 玻璃卡 padding 24→16px，内容区上边距 24→48px，控制台顶部明显低于窗口右上角关闭按钮（卡片顶 y≈60 < 叉号底 y≈40）。

### 系统集成轮（同日跟进）

- 标题栏刷新/设置按钮从右上 `pack_end` 改为左上 `pack_start`（用户反馈"右上角按钮放左边"）。
- 实测特权链路发现并修复 unit 模板 bug：systemd `WorkingDirectory=` 不接受引号（`ExecStart` 接受），`src/privileged.rs` 的 workdir 去掉 `systemd_quote`，helper 重部署后 `enable-service` 实测成功。
- 本机补齐 polkit 认证代理（`polkit-gnome` + niri `spawn-at-top-startup`），`pkexec` 授权弹窗实测通过；此前 GUI 特权操作（自启开关/重启服务/安装）静默失败。
- 服务启动实测发现官方 2014 闭源客户端在 root 下 SEGV（与现代 glibc 不兼容），已停止开机自启防崩溃循环；「打开实时日志」终端此前为空属正常（journalctl 按 unit 过滤、服务从未运行）；日志链路用崩溃栈历史实测确认工作正常。

### 舞台+控制台重设计轮（同日跟进）

- 删除全部嵌套玻璃小卡与 2×2 指标网格，改为**舞台 + 控制台**：左侧透明舞台直接透出樱花场景（大状态字胶囊 + 自绘链路图 + 4 状态胶囊 pill），右侧 420px 单张玻璃控制台（连接设置表单 / 开机认证 / 连接断开安装 / 最近日志 / 诊断 5 行）。
- 断点：<760 控制台 fill+solid 实底 + 顶部紧凑状态条，760-939 紧凑舞台，≥940 完整舞台。
- 文字对比度统一招式：GTK4 无 text-shadow，舞台大字/区块标题/表单 label/headerbar 标题统一"深紫玻璃胶囊 alpha(#4A3048) 托底 + 白字"，玻璃卡底 alpha 提至 0.48-0.64；popover 由白玻璃改为深紫玻璃（user 反馈白浮层"字体和阴影很奇怪"后重做）。
- 链路图节点图标经用户反馈替换为 Tabler `device-laptop`/`server`（随节点半径缩放）。

### 布局重设计轮（同日跟进）

- 按"行动与感知分离"重构主界面（参考 GNOME Connections、ssh-client-manager、adw-network 布局）：左侧玻璃卡片改为**纯操作区**（标题 + 连接设置表单 + 连接/断开/安装 + 最近日志行），右侧新增**监控面板**（运行状态大字 + 2×2 状态指标网格 + 诊断与工具 5 项）；双栏断点阈值从 1080 降到 940，Niri 半屏 954px 即进入双栏，消除"500px 窄条竖排清单 + 右侧大片空白"的空洞感。
- 表单改为 icon + 等宽 label + 输入框的两列对齐行（`.form-line`/`.form-label`），替代纵向 ActionRow 清单；状态四格改 2×2 网格（`stat_grid`，column_homogeneous）。
- 日志不再常驻右侧面板（避免纵向溢出、左轻右重），由主卡底部"最近日志"行承载，点击弹出完整日志浮层；窄屏（<940）显示摘要状态区（`narrow_status`）。
- 纵向压缩：big-state 20pt→17pt、glass-section/stat-cell/stat-badge padding 收紧、side-panel 行 min-height 34px；默认窗口 1280×820。
- 修复浮层白圈根因：libadwaita 默认 `popover > contents` 的 1px 边框 + box-shadow 未覆盖，浮层边缘叠出白色光圈，补 `border:none; box-shadow:none`。
- 修复窗口级 CSD 边框残留：libadwaita `window.csd` 默认 1px 白色 outline（`RGB(255 255 255/7%)`）与 `window.csd.tiled` 的 1px color-mix 边框（Niri 平铺触发），统一 `box-shadow:none; border:none; outline:none`（含 backdrop/tiled/maximized/fullscreen 变体），窗口外亮色带消除。
- 954×820 双栏、640 窄屏单栏、1280 宽屏双栏经 Gemini 逐屏评审（9.5/10：完整无截断、左右协调、无白圈/亮橙边框）；build/clippy/fmt/test 全绿；已重装本机（哈希 7bc8398d）。

### Changed

- 删除旧四工作区前端（连接/日志/诊断/设置与暖白侧栏），重建为暮色学园单窗口皮肤：自绘场景层（靛蓝→暖橙渐变天空、呼吸星点、月亮、学园建筑剪影、窗边少女剪影、薄雾）+ 玻璃拟态表单卡片。
- 三种叙事动效：认证中底部暖橙光带升起、成功全画布霞光漫开、失败冷青闪击后回落；按钮流光与卡片 hover 使用声明式 CSS 过渡。
- 适配 Niri 三档列宽：640 窄列卡片铺满且背景零装饰，960 卡片偏左并让剪影从卡片右缘外开始，1280 完整暮色学园构图；断点阈值 760/1080。
- 修复刷新状态时误触发开机认证服务启停（新增 refreshing 标志隔离 notify 回调）；修复开关被 ActionRow 行高纵向拉伸；浮动缩窗高度不足时内容可滚动。
- 保持后端与提权链边界不变；`Cargo.toml` 的 gtk4 feature 从 `v4_10` 升级到 `v4_20`。

### 樱花皮肤轮（同日跟进）

- 视觉方向从暮色切换为樱花学园：场景素材换为 `data/scene/scene-sakura.png`，配色系改樱粉白磨砂（卡片渐变、深紫文字、粉按钮）。
- 标题栏改为沉浸式：`AdwToolbarView` 结构替换为根 Overlay（场景铺满全窗口为底，卡片 ScrolledWindow 与透明 headerbar 浮层），背景贯穿到窗口顶部，标题做成白底胶囊徽章。
- 图标与组件精修：引入 Tabler 线性图标（表单前缀、连接/断开/安装、刷新、日志），统一玫瑰粉描边 `#b8507c`，以 `gdk::Texture::from_bytes` 从内嵌 PNG 构造；状态四格改为圆形粉渐变徽章 + 图标 + 名称 + 状态值；日志行、输入框、开关、行 hover 同步细化；副标题/占位符对比度整体提升。
- 修复 headerbar 渐变不生效的根因（`background-color` 未置透明导致 libadwaita 底色叠加成纯白条）。

### 阶段 3 渐进披露浮层（同日跟进）

- header 新增"更多工具"齿轮按钮：浮层（`gtk::Popover`）收纳低频项——"连接设置"组（DHCP 自动获取 IP、保存密码两个开关，读 `config::load()` 初值、变更即存 `settings.conf`，替代原先硬编码值）+ "诊断与工具"组五项（测试网络连通 ping 223.5.5.5、重启开机认证、打开客户端目录、打开实时日志、在线帮助），点亮 `system.rs` 原 dead_code 入口。
- "最近日志"行改为可点击：弹出完整日志浮层（等宽文本、可滚动），主卡片只保留核心认证流程与状态四格。
- 新增 displacement Tabler 图标（settings、terminal-2、folder-open、help-circle、bulb），与既有图标同一玫瑰粉语言。
- 三档截图经 Gemini 复审：无大白条、浮层与卡片完整、配色协调。

### Verification

- Rustfmt、Rust 测试（11+10+3）、Clippy、Release 构建全部通过；ShellCheck、desktop/XML 校验和两套 `/tmp` 隔离回归未受影响。
- 在真实 Niri 会话中完成 640、960、1280 三档截图、Gemini 逐屏评审与像素级定量验证；真实校园认证仍待实机验证。
- 樱花皮肤轮：1280/960/640 三档截图经 Gemini 评审（9.0/10）与 640 窄列逐项验收，无布局缺陷；已知遗留：GTK CSD 窗口 buffer 固有约 15px 透明外扩（非本应用可去除）。


## 0.3.0 - 2026-07-15

### Added

- 在官方客户端缺失时，通过原生文件选择器安装学校提供的 Linux ZIP。
- `scripts/install.sh --uninstall`，停止服务并清理安装产物，同时保留用户设置。
- Arch Linux GitHub Actions，自动执行 Rust、Shell、desktop、XML 和安装卸载回归检查。
- ZIP 安装、wrapper 生成、失败回滚和不安全路径的 Rust/脚本测试。
- 固定在 `/usr/lib/rjsupplicant-gui` 的 root-owned helper，以及按六个白名单子命令匹配的 polkit policy。
- GitHub curl 一键引导安装，自动下载并校验学校官方 Linux V1.31 客户端。

### Changed

- 将 GTK 断点、导航、运行状态和通用组件拆分到 `src/ui/` 子模块。
- GUI 与命令行安装均先准备新客户端和 wrapper，再替换旧安装。
- 官方客户端与 wrapper 迁移到 root-owned `/usr/lib`；旧用户级路径仅作为升级回退。
- 开机认证 service 由 helper 按当前设置生成，安装脚本不再创建参数不完整的默认服务。
- 重复运行安装器时跳过已经就绪的 root-owned 客户端，可通过环境变量显式要求重装。
- Arch 安装时复用已有 rustup 工具链，避免与 pacman 的 `rust` 包发生 cargo 冲突。
- 将“设置中心”由路径提示改为可编辑的原生偏好设置对话框，并增加 `Ctrl+,` 快捷键。
- “关于”对话框直接读取 Cargo 包版本。

### Security

- 解压前拒绝绝对路径、反斜杠逃逸和 `..` ZIP 条目。
- 从单一文件句柄创建 ZIP 快照，拒绝符号链接源、归档符号链接和特殊文件，并收紧解压权限。
- helper、客户端、systemctl、unzip、wrapper 解释器和架构检测均使用固定 root-owned/系统路径。
- wrapper 路径使用 shell 单引号转义，且不继承调用方 `LD_LIBRARY_PATH`。
- ZIP 安装每次要求管理员授权；日常保留授权只执行严格解析的 helper 白名单动作。
- helper 在停止、禁用或重启前拒绝引用用户路径、可写文件或环境注入的旧 service；启用时先原子重写固定 service。
- 安装脚本的系统路径覆盖仅在显式测试模式下允许，并限制在 `/tmp`。
- GUI 通过标准输入向 helper 传递密码；helper 参数不再包含密码，终端回退输入关闭回显。
- bootstrap 拒绝覆盖非 Git 目录、本地修改或分叉分支，并固定校验学校 ZIP 的 SHA-256。
- 安装时使用锁定依赖重新生成最终 GUI/helper 输出，不信任已有 release 二进制。
- 新客户端或 wrapper 安装失败时恢复旧客户端目录。

## 0.2.0 - 2026-07-14

- 重做面向 Niri 四档列宽的暖色桌面界面。
- 修复官方客户端 fork 后 systemd `Type=simple` 立即断开的问题。
- 分离客户端、认证进程、有线链路和开机认证状态语义。
- 将授权、systemctl、日志和状态读取移出 GTK 主线程。
- 增加账号/网卡校验、systemd 参数转义和 `0600` 配置权限。
