# Changelog

## Unreleased - 2026-09-02

## Unreleased - 2026-09-02

## Unreleased - 2026-09-02

### 状态可信度、渲染资源与提权纵深轮（第三批）

- **认证结果不再把"客户端已退出"当成功**：helper 的判定优先级改为进程已退出 > 认证成功 > 失败标记后，早退分支返回 `Err("认证未完成：官方客户端进程已退出")`。依据是本项目自己已实机确认的事实——认证成功时客户端保持前台持会话（开机自启 unit 因此用 `Type=simple`），进程消失即会话不存在；退出码仍不参与判定（实测 DHCP 失败也返回 0）。
- **helper 特权动作加锁**：`/run/rjsupplicant-helper.lock` 上的 `flock(LOCK_EX|LOCK_NB)`，拿不到立即报"另一个特权认证操作正在进行"。此前两个实例会并发跑两个 root 客户端抢同一网卡，并交叉读取同一份 `run.log` 做成败判定（把对方的结果当本轮）。`restore-network` 由 systemd 以 root 调用，**不参与加锁**，否则开机认证的 NM 恢复会被界面动作挡住。
- **场景与链路不再无限重绘**：`Scene`/`Link` 的 tick 回调原先把控件克隆进自己被控件拥有的闭包（自引用环，控件永不释放）且无条件 `queue_draw`。改为只用回调自带引用、`is_mapped()` 门控、Idle 30fps 帧预算、链路 Idle 直接 `Break` 退出帧时钟；`Link` 与 `Scene` 现在都按同一组纯函数结算动画终点，成功后各自回落静态画面（原先 `Success` 的全画布粉层与链路光点会永久驻留）。
- **薄雾层不再破坏绘制矩阵**：`draw_mist` 原先用 `cr.identity_matrix()` 收尾，抹掉 GTK 准备的控件变换（含 HiDPI/分数缩放），而它画在模式特效层之前，导致之后的光带/霞光/闪击按设备像素绘制错位；改为 `save()/restore()`。
- **UI 状态可信度批量修复**：`run_backend` 的"连接失败"大字只对连接类动作生效（安装客户端失败不再伪装成认证失败）；忙碌期间一并禁用迁移横幅按钮（此前可点、选完文件被 busy 静默吞掉）；账号/密码框回车等同点「连接」；服务胶囊区分 `activating`（ExecStartPost 的 8 秒节点内不再误报"服务 异常"）；网卡列表每轮刷新重新探测并通过 `splice` 重建下拉框、尽量保留用户选择（此前是构建期快照，后接网卡选不到也看不到）；选择器拿到非本地路径时给出可操作提示而不是静默无反应；无 Display 时不再 `unwrap()` panic；胶囊初值不再先亮"健康"绿点。
- **开机认证与保存密码的耦合显式化**：unit 里没有口令，`save_password=false` 时启用开机认证必然开机失败；现在开关会直接回弹并说明原因，而不是留下一个永不认证的 unit。
- **配置读写更严格**：布尔项只认 `true/1/false/0`（旧写法 `!= "false"` 把 `0/no/off/空/拼错`一律读成 true），无法识别时保持默认；`save` 改为临时文件 + `sync_all` + `rename` 原子替换（旧 `create+truncate` 就地改写，中途被杀会留下半截配置并在下次加载时静默回落默认，等于账号丢失）；`XDG_CONFIG_HOME=""`/`XDG_DATA_HOME=""`/`HOME=""` 按未设置处理，不再拼出依赖当前目录的相对路径。
- **架构判定收拢为单一来源**：`privileged::client_arch_dir()` 用 `env::consts::ARCH` 判定（指针宽度在 aarch64 上同为 64 位，会错误指向不存在的 `x64`），GUI 配置路径、helper、客户端安装三处共用；无法识别时返回必然不存在的目录名使"已安装"判为假，`client_install` 则明确报"官方客户端只提供 x86 版本"。`scripts/install.sh` 的 `getconf LONG_BIT` 同步换成 `uname -m` 分支。
- **systemd 单元校验补白名单**：`service_content_uses_owned_paths` 过去只看 `Exec*`/`WorkingDirectory=`/`Environment*`，其余指令原样放行，`User=`/`BindPaths=`/`RootDirectory=`/`StandardOutput=file=`/`OnFailure=` 都会被判"安全"；现在未知指令一律拒绝，注释与 `[Section]` 行仍放行（各有回归测试）。
- **清理与诚实标注**：删除已无引用的 `.log-text`/`.row-badge` CSS 与重复的 `.console row .title/.subtitle` 规则（合并后逐属性一致）；`tests/install_uninstall.sh` 里 `UNSAFE_ZIP`/`ROLLBACK`/`FAIL_HELPER_INSTALL` 三个用例补写它们真实的覆盖边界（只验证 install.sh 在 sudo 步骤失败时非零退出且不删文件，ZIP 净化与回滚的真实覆盖在 `client_install.rs` 单测）。
- README 新增「已知安全边界」，如实写明两处无法在本仓库消除的暴露（官方客户端只接受命令行传口令；客户端把设置写在他用户可读的工作目录）以及多用户机器的 `hidepid=2` 处置建议；并修正"install.sh 会配置开机认证文件"的错误描述（unit 由应用内开关生成）。
- **CI 最后一步在容器里必然失败（main 已红两次）**：`verify.yml` 的 `Check whitespace errors` 跑 `git diff --check`，但 Arch 容器内 `git: command not found`（exit 127），`454466f` 与 `f476d85` 两次运行都因此失败——其余步骤全绿。修复：依赖清单显式加 `git`，checkout 设 `fetch-depth: 2`，并把该步改为与父提交比较（无父提交时退回原行为）。原来的 `git diff --check` 即便有 git 也是在干净 checkout 上做空比较，永不失败。
- 验证：44 项 Rust 测试（较上批 +3）、`clippy --all-targets -D warnings`、`fmt --check`、`cargo build --release`、`bash -n`、`shellcheck` ×4、`desktop-file-validate`、`xmllint`、`git diff --check`、`tests/bootstrap.sh`、`tests/install_uninstall.sh` 全绿；已部署本机（GUI/helper 哈希与构建产物一致），部署后实机启动测量稳定态 CPU、线程数与 stderr（见 HANDOFF 验收记录）。像素级截图与三档宽度评审在本机媒体护栏下不可用（`grim` 输出图片路径被 `PreToolUse` 拒绝，按约定不绕过），CSS 改动改用规则集逐属性比对证实等价。
- 仍未解决（有明确理由，不是遗漏）：H1 口令进官方客户端 argv（闭源程序只有 `-p`，只能靠 `hidepid=2` 或文档告知）；H2 root 客户端目录 0755 与凭据落盘（收紧到 0750 会让 GUI 读不到 `run.log`，需引入专用组并重登录才两全，代价与收益需你定）；慢网络超时无判定路径仍返回成功且不杀客户端（区分不了"将成功"与"卡死"）；`ForceDark` 与"不套 ScrolledWindow"是既有冻结决策，未动。

### 并发与资源修复轮（第二批，紧接提权边界轮）

- **轮询不再无上限开线程**：`refresh_status` 每 10 秒由定时器触发一次并 `std::thread::spawn` 一个工作线程执行 `load_status()`（内部串行跑 `systemctl is-enabled`/`is-active`、`nmcli radio`、`journalctl -n 60`、`ps`、多次 `ip`，全部无超时）。旧实现没有在途判断，一旦 systemd 总线或 journal 卡住，线程与子进程按 10 秒一条无限累积。新增 `refresh_status_polled` 作为定时器入口，在途则跳过本次；**手动刷新与动作完成后的刷新仍直接走 `refresh_status`**，所以即使某轮真的卡死，用户点刷新仍可恢复，状态不会被永久锁死。
- **认证日志读取偏移前进**：`new_log_tail` 原先每次 200ms 都从固定起点重读到文件末尾（实测 `run.log` 跨运行追加、永不轮转，已 150 行/2 个日期），既造成二次方读放大，也让判定窗口覆盖整段历史新增。改为推进偏移，并处理两类此前会永久失效的边界：文件变短（清空/轮转）时重置偏移从头读；含非法 UTF-8 字节时跳过该段（旧实现返回 `None` 后偏移不动，此后每轮都在同一处失败，成败判定永久失效并退化为超时假成功）。新增真实文件回归测试覆盖追加、偏移前进、截断复原、坏字节后继续判定四种情形。
- **判到失败即终止客户端**：`AuthOutcome::Failed` 时客户端仍在运行（早退会被 `ClientExited` 先捕获），此时它既不会建立会话、argv 里又带着 `-p` 明文口令，属于纯粹的凭据驻留面，故 `kill` + `wait` 回收。**超时路径故意不杀**：慢网络上"再过一会就成功"与"卡死"无法区分，误杀会真打断一次能成的认证；该路径仍会留下持有口令的孤儿进程，H1 与"假成功"（`Ok(())`）在这条路上未修。
- 未开工并有意推迟：**helper 单实例锁（M12）**。加锁需先决策 `ExecStartPost` 的 `restore-network` 是否参与竞争——它与交互认证共用同一把锁会让开机认证的 NM 恢复被 GUI 动作挡住；`install-client` 与 `authenticate` 的锁粒度也不同。设计未定前不硬加。
- 验证：33 项 Rust 测试（13 + 13 + 7）、`clippy --all-targets -D warnings`、`fmt --check`、`cargo build --release`、`bash -n`/`shellcheck` 全绿；已部署本机（GUI 与 helper 哈希与构建产物一致），实机启动窗口"锐捷有线认证"正常出现、跨 3 个轮询周期线程数稳定为 22、子进程 0 泄漏、stderr 零输出（无 GLib/CSS 告警）。未验证：真实认证链路上的偏移前进与失败终止（需重新认证，会打断当前会话）。

### 提权边界与认证清理修复轮（三路并行深度审计后开工）

- **认证早退漏恢复网络（真实隐患）**：`rjsupplicant-helper` 的 `authenticate` 原先在客户端 8 秒内退出（网线未插、认证服务器不通、崩溃）或判定失败时直接 `return`，**跳过 `restore_network_services()`**，而客户端此时已按自身设计停掉了 NetworkManager——用户会同时收到"成功/已报告"的反馈和本机无线被静默切断的后果。重构为单一退出循环 + `NetworkRestorer` RAII 守卫（与既有 `TerminalEchoGuard` 同模式），8 秒 DHCP 注入节点保留显式恢复；判定逻辑提取为纯函数 `classify_auth`（优先级：进程已退出 > 认证成功 > 失败标记，后者避免官方提示里的"认证失败"字样把真实成功翻成失败）。
- **卸载不再以 root 执行用户可写文件**：`scripts/install.sh` 的 `disconnect_running_client` 原先在无 root-owned 客户端时直接 `sudo ~/.local/bin/rjsupplicant -q`——该路径归普通用户所有，任何能写家目录的进程替换它即可借一次卸载拿到 root，与 `AUDIT.md` 自订的"不把提权授权授予用户可写 wrapper"原则矛盾。新增 `is_root_owned_executable` 闸门：非 root-owned 或组/其他可写即拒绝并给出可操作说明。
- **提权目标统一闸门**：`system::run_elevated_wait_with_input` 入口新增 `ensure_root_owned_program`，pkexec 与终端 `sudo` 两条路径都只接受 root-owned 且他人不可写的程序（与 helper 侧 `is_secure_root_executable` 同一判据）。行为变化：legacy 用户级 wrapper 不再被提升到 root，改为明确报错引导重装 root-owned 客户端（迁移横幅早已指向同一动作）。
- **终端回退不再谎报成功（行为兼容性修复）**：无 polkit 时的 `sudo` 终端回退原先 `spawn()` 后即返回 `Ok(())`，拿不到退出码与 stderr，任何失败都显示为完成。改为等待终端进程结束（kitty/foot/alacritty/xterm 均在被指挥命令退出后才关闭）并传播非零状态；`run_terminal` 拆为 `spawn_terminal` + `run_terminal`，`journalctl -f` 等不等待用法行为不变。等待与 pkexec 共用新提取的 `wait_for_child`（同一 `ELEVATION_WAIT_TIMEOUT` = 120 秒上界，超时 kill + reap 直接子进程并报错）——否则"改为等待"会引入新的不对称：密码提示挂起时 GUI 永久忙碌。已知限制（两条路径相同）：`kill` 只作用于 pkexec/终端模拟器本身，其派生的 root 侧进程不被回收。
- **`command_exists` 绝对路径分支补执行位判定**：原先只 `Path::exists()`，目录或无 x 位的 `/usr/bin/pkexec` 会被当成"可用"，绕过上面的显式失败。现与 PATH 查找分支共用 `is_executable_file`。
- **安装脚本前置权限检查与构建隔离**：`preflight_privileges` 在任何破坏性动作（`rm -f target/release/*`、`cargo build`）之前确认 `sudo` 可用；生成物路径改为尊重 `CARGO_TARGET_DIR`（`BUILD_DIR`），`tests/install_uninstall.sh` 据此在临时目录内构建并设 `RJSUPPLICANT_KEEP_BUILD=1`——此前该"隔离回归"测试会对真实仓库执行 `cargo build --release` + `cargo clean`，清空开发者或并行 Agent 的 `target/`。
- **回归测试清理 trap 加固（防御性）**：`tests/install_uninstall.sh` 的 `trap 'rm -rf "${TMP_DIR}"' EXIT` 改为统一 `cleanup` 函数并覆盖 `EXIT HUP INT TERM`。诚实限定：观察到两次被工具超时终止的运行在 `/tmp` 各留下 393M 临时树，但最小复现表明普通 SIGTERM 下旧的 EXIT trap 也会执行，真实成因是宽限期后的 SIGKILL（任何 trap 都拦不住）；本改动只对"只收到 TERM/INT 的父进程"这一类场景有效，不宣称修复已观察到的残留。已手工清掉那 786M。
- 验证：32 项 Rust 测试（新增 `classify_auth` 两条、提权闸门两条、`command_exists` 执行位一条、提权有界等待一条）、`clippy --all-targets -D warnings`、`cargo fmt --check`、`bash -n`、`shellcheck`（四个脚本）、`tests/bootstrap.sh`、`tests/install_uninstall.sh` 全绿。H3 缺陷先用独立探针在**未修复代码**上复现（`sudo /tmp/.../home/.local/bin/rjsupplicant -q` 确被派发）后再修复复验；`is_root_owned_executable` 在真实 uid 与 `unshare -r` 的 uid=0 两个视角下分别验证拒绝/放行用例（首轮发现的 `$((8#755 & 0o022))` bash 底数错误已修正为 `8#022`，该 bug 会让合法 root-owned wrapper 也被拒绝）。回归后确认仓库 `target/`（1.3G / 279 个 debug deps）未被清空。
- 未修记录备查（本批未开工）：helper 超时/失败不 kill 不 reap、`log_offset` 不前进与 `run.log` 永不轮转、无单实例锁、`ClientExited`/超时仍返回 `Ok(())`（假成功）、root 客户端目录 0755 与运行时凭据落盘、UI 状态与渲染批次。

### 重启失联事故修复轮（快照恢复前取证，三个子代理并行深度审计）

- **开机自启 service 单元 Type 修复（真实隐患）**：实测官方客户端认证时保持前台运行（多轮实机实证 PPID 不脱离），原 unit 的 `Type=forking + GuessMainPID=yes` 一旦真正启用必在 `TimeoutStartSec=30` 超时失败、`ExecStartPost`（恢复 NM）永不执行，重启后必然断网。改为 `Type=simple`：客户端即服务主进程、持有会话；`ExecStop` 的 `-q` 正常断连；崩溃时 `Restart=on-failure` 拉起；验证器与测试同步。
- **GUI 增加「Wi-Fi 已禁用」提示**：`load_status` 新增 `wifi_radio_enabled`（`nmcli -t -f WIFI radio`），副行在射频关闭时追加「· Wi-Fi 已禁用」，让用户第一时间理解连不上 Wi-Fi 的真实原因。
- **事故归因（全部实证，子代理三路交叉核验）**：
  - 开机自启 unit **从未被创建**：全 journal 无 `enable-service/disable-service` 的 pkexec 记录；「估计自启有问题」的直觉正确——启用即会踩中上方 Type 缺陷，已一并修复。
  - 「重启后一直连接失败」无客户端日志支持：重启后唯一一次 09:41 认证即成功；NM 停止 → 8 秒恢复为 helper 既定时序（09:41:02 stop → 09:41:10 start，NM unit 为 `Type=dbus + Restart=on-failure`，显式 stop 不触发自动重启）。
  - 「Wi-Fi 连不上 / 网络设置直接不见」：用户 09-01 18:27 手动关闭 Wi-Fi，systemd-rfkill 跨重启恢复软阻塞（wlan/bluetooth 归档值 0），DMS 设置前端在射频关闭时按设计隐藏全部 Wi-Fi 内容（`NetworkWifiTab.qml`）；**四个连接档案全部完好**（`/etc/NetworkManager/system-connections`），`NetworkManager.state` 的 `WirelessEnabled=true`；射频已恢复（`nmcli radio wifi on`）。
- 验证：27 项测试、clippy `-D warnings`、release 全绿。

## Unreleased - 2026-09-01

### 实机联调修复轮（校园有线网实测，用户在场配合授权）

- **启动卡死修复（主线程阻塞根因）**：`run_backend`/`run_diag`/`run_service_toggle`/`run_backend_quiet`/`refresh_status` 的阻塞调用（pkexec 等待、systemctl、客户端运行、状态读取）原先在 `glib::spawn_future` 主上下文线程上执行，阻塞体冻结整个 UI（实测：点击连接后界面卡死）。改为真实工作线程 + `futures-channel` 异步回传结果（新增直接依赖 `futures-channel`/`futures-util`，均在 glib 依赖树内、零新增编译），UI 更新仍在主线程。
- **空密码复用已保存密码**：GUI 连接不再拦截空密码（后端本就支持客户端复用已保存密码），留空时 toast 提示。
- **连接后热点 Wi-Fi 断连（根因实证）**：strace 证实官方客户端启动时主动执行 `systemctl stop NetworkManager`（其设计行为，非本项目代码）。helper 在认证/断开/自启/重启等所有客户端运行过的动作后自动恢复 NM；开机认证 unit 增加 `ExecStartPost="…rjsupplicant-helper" restore-network`（新增白名单动作，systemd 直接以 root 调用，无需新增 polkit 条目），安全校验器同步接受该行（旧 unit 需重新启用触发迁移）。
- **认证成功但拿不到 IP（根因实证）**：pcap 证实客户端内置 DHCP（2014 二进制）在现代内核上**不发任何报文**；802.1x 实际成功、端口放行、学校 DHCP 正常（48 小时租约）。helper 认证流程改为：启动客户端约 8 秒后恢复 NM，由 NM 的内部 DHCP 获取地址并建立 eno1 默认路由，客户端轮询 `/proc/net/route` 确认后认证成功并保持会话（实机：4 秒认证成功、helper 8 秒返回、会话保持、NM 恢复、eno1 获 `192.168.129.140/23`）。
- **helper 认证结果判定重写**：客户端退出码不可靠（DHCP 失败也返回 0），改为轮询官方日志新增行判定（成功=「认证成功」；失败=`网线没有连接上 / 无法连接认证服务器 / 认证失败 / 无法获取动态IP地址` 并立即返回对应错误）；成功后立即返回、不阻塞到会话结束（避免 GUI 的 pkexec 120 秒超时杀掉已认证会话）；最长 60 秒兜底。
- **安装依赖补 net-tools**：客户端依赖 `ifconfig`（strace 见 7 处调用，缺失使「正在启用网卡」失败）；`scripts/install.sh` 依赖清单加入 net-tools。
- 验证：27 项 Rust 测试、clippy `-D warnings`、release 构建、shell 回归（安装/卸载 + bootstrap + ShellCheck）全绿；实机 {认证成功 + 中文欢迎横幅 + 断开后 NM 保持 + Wi-Fi 恢复} 全部验证通过。

## Unreleased - 2026-08-16

### 安装后清理原则（文档）

- HANDOFF.md 新增「安装后清理原则」小节：明确安装脚本只自动删除可再生成的编译缓存（`target/`，`cargo clean`，`RJSUPPLICANT_KEEP_BUILD=1` 跳过），保留源码、官方客户端 ZIP、用户设置、程序本体与运行状态；附通用判据三问与可原样转述给其他项目的一段话。
- scripts/install.sh 的 `cleanup_build_artifacts` 补充清理/保留边界注释；行为不变。

## Unreleased - 2026-08-03

### 交互反馈修复轮（同日跟进，用户反馈"鼠标放右边面板变手状但点击无效果"）

- **根因**：`set_busy` 只禁用了连接/断开/安装/刷新 4 个按钮，诊断行/日志行/自启开关/网卡下拉在忙碌期间仍是可点外观（手状光标），但点击处理开头会被 busy 保护直接吞掉——表现就是"hover 是手状、点击没反应"。
- 修复①：忙碌期间禁用全部交互控件（表单、开关、下拉、日志/诊断行），控件变灰且不再显示手状光标，状态一目了然。
- 修复②：pkexec 授权等待加 120 秒超时（此前无超时，用户不响应 polkit 弹窗时 GUI 永久停留在忙碌状态、所有点击被吞）。

### 固定布局轮（同日跟进，用户反馈"页面可以滚动，要固定主/下面往上拉"+"日志和诊断都有打开实时日志，清除一个"）

- 移除「打开实时日志」重复入口：诊断与工具区由 5 项减为 4 项（测试连通/重启开机认证/打开客户端目录/在线帮助），实时日志入口保留在日志区块。
- 取消页面滚动：删除包裹内容的 `ScrolledWindow`，控制台内容压缩到固定高度直接铺在场景层上（不再出现滚动条）；控制台进一步压缩（spacing 3→2、行高 26→22、标题 21→19pt、按钮 padding 收紧），默认窗口高度 820→860，保证内容在平铺与初始窗口内完整显示。

### 状态实时刷新（同日跟进，用户"能实时显示吗，而不是硬编码"后执行）

- 所有认证动作完成后无条件 `refresh_status`：连接成功/失败、断开、诊断、安装、自启开关操作后，连接详情区（状态/时长/最近认证）、IP、服务胶囊立即反映真实结果，不再需要手动点刷新。
- 新增 10 秒轻量轮询（`timeout_add_seconds_local`）：「已连接 X 分钟」、IP/网关、服务状态持续更新；认证动作进行中（busy）自动跳过。

### 连接详情区（同日跟进，用户选择"日志下方空白加连接详情"）

- 控制台底部新增「连接详情」区块（日志下方）：认证账号（当前输入）、连接状态（含已连接时长）、最近认证结果（成功/失败 + 相对时间，会话内记录）。
- 认证结果记录：连接失败在后台闭包记录失败信息，连接成功在 run_backend 成功分支记录，刷新时以「成功（刚刚）/失败：原因（N 分钟前）」展示。
- 默认窗口高度 860→900（内容增加后保证完整显示）。
- 样式调优（用户"字大一点、往下放、字体颜色意见"）：字号 9→10pt、区块下移与日志预览分开；Pango markup 单行分色——标签浅紫灰、值深色，连接状态已连接粉/未连接深灰、最近认证成功绿/失败红加粗（值经 `glib::markup_escape_text` 转义）。

### 网络详情（同日跟进，用户选择"面板加 IP/网关"）

- 控制台网卡行下方新增网络详情小字：当前选中网卡的 IPv4 地址与默认网关（`IP 192.168.x.x · 网关 …`），无地址时显示「未获取到 IP」——认证成功后"拿到 IP 没"最直观。
- 后端新增 `system::interface_ipv4`/`interface_gateway`（`ip -4 -o addr show` + `ip route show default` 解析），随状态刷新更新。

### 日志区块精简（同日跟进，用户"查看完整日志点击没反应/没有效信息"后执行）

- 移除「查看完整日志」行与完整日志浮层（TextView+Popover）：预览区已覆盖快速查看，完整日志可随时用「更多工具」里的「打开实时日志」终端查看；日志区块仅保留「日志」标题 + 最近 4 行预览。
- 顺带清理：`ICON_LOG`、`.log-text` 样式等死代码移除；refresh_status 元组去掉不再消费的 log_text。

### 日志预览区（同日跟进，用户"面板显得空"后执行）

- 控制台「日志」区块新增**最近日志预览**：等宽字体显示最近 4 行日志（白色玻璃托底），随状态刷新实时更新；「查看完整日志」行保留（点击弹完整日志浮层，数据源同为 journalctl 尾部 + run.log 尾部）。
- 控制台内容从 ~570px 填充到 ~750px，底部留白大幅减少，默认 860 窗口完整显示。

### 诊断工具收进浮层轮（同日跟进，用户反馈"为什么设计成点击/手状但点击无效果"后执行）

- **修复控制台诊断行"手状但点击无效"根因**：控制台底部的诊断行此前设置了 pointer cursor 却从未绑定点击事件（`Ui.diag` 绑定的是浮层内行），点击自然无反应；现已删除控制台底部整块「诊断与工具」区。
- **诊断工具统一收进「更多工具」浮层**：测试连通 / 重启开机认证 / 打开客户端目录 / 打开实时日志 / 在线帮助 5 项与连接设置（DHCP/保存密码）一起在浮层中管理，均绑定动作；控制台只保留核心认证流程（账号/密码/网卡/自启/连接断开/安装/查看完整日志），页面固定无滚动且更简洁。
- 日志区块精简：「打开实时日志」行移除（浮层内有），仅保留「查看完整日志」。
- 控制台间距从压缩态适度恢复（spacing 2→6、行高 22→26、标题 20pt），内容约 570px 高度在 860 默认窗口中完整显示、底部留白透出场景。

### 深度审查修复轮（同日跟进，用户要求剖析隐患/边界/bug 后执行）

- **修复连接状态误报**：`run_backend` 区分连接类与非连接类成功态——「安装客户端」「启用开机认证」此前复用连接动作的成功分支，会把舞台大状态字显示为「已连接」并触发成功霞光；现改为非连接类成功后回到平静场景并刷新真实状态。
- **自启开关失败回滚**：`enable-service`/`disable-service` 失败时开关位置不再与服务真实状态脱节（回滚 + 刷新同步）。
- **systemd 启动限流**：生成的 unit 增加 `StartLimitIntervalSec=60`/`StartLimitBurst=3`，防止官方客户端崩溃时 `Restart=on-failure` 无限重启刷日志（实测：新 helper 生成的 unit 已含限流，SEGV 场景下启动 3 次后停止重试）。
- 验证：fmt/clippy/24 测试全绿；helper 与 GUI 已重部署（helper 哈希 8e3809d9 系）。

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
