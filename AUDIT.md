# 客户端实现审计

- 审计日期：2026-07-15
- 审计版本：0.3.0

## 结论

当前实现已经满足“Arch Linux 上的官方锐捷有线认证桌面前端”这一定位：可以恢复安装、配置认证参数、连接/断开、管理开机认证、查看状态和诊断日志，并且没有自行实现或伪造锐捷协议。需要 root 的新架构操作统一经过固定 root-owned helper，不再把保留 polkit 授权授予用户可写 wrapper。

它不是跨发行版的通用网络管理器，也无法绕过官方闭源客户端本身的兼容性和接口限制。

## 本轮已修复

### 严重：systemd 服务模型错误

官方客户端会自行进入后台，旧单元却声明为 `Type=simple`。启动器退出后，systemd 会立即执行 `ExecStop`，导致“服务已启用”但认证实际被停止。

处理：改为 `Type=forking`，增加启动/停止超时，并让断开操作在服务运行时使用 `systemctl stop`，避免被重启策略重新拉起。

### 严重：服务参数缺少边界保护

账号和网卡曾直接拼入 `ExecStart`，空白、引号或 systemd `%` 说明符可能改变参数解析。

处理：限制账号和网卡字符集，统一引用并转义 systemd 参数，新增相应用例。

### 主要：状态语义不准确

旧界面把 `systemctl is-active` 近似描述为网络已经认证，但手动连接并不由该 service 托管，service active 也不等于认证成功。

处理：从 `/proc` 检查真实认证进程，读取网卡 carrier，并独立展示客户端、进程、链路和自启状态。最终认证结果明确以日志为准。

### 主要：授权操作冻结或过早提示成功

部分操作只执行 `spawn()` 就提示成功，另一些 `status()` 又直接阻塞 GTK 主线程。

处理：所有可能阻塞的授权、systemctl 和状态读取进入后台任务；拿到退出状态后再反馈，并延迟刷新进程状态。

### 主要：网卡枚举范围不正确

旧实现把除 `lo` 外的无线、虚拟和元接口全部列为“有线网卡”。

处理：根据 sysfs 的接口类型、`wireless` 标记和物理设备节点筛选，物理网卡不存在时才回退到其他以太网接口。

### 一般：日志来源不完整

一旦 journal 中有旧记录，手动认证产生的官方 `run.log` 就不会显示。

处理：合并官方客户端最近 80 行与 systemd 最近 60 行。

### 一般：桌面完成度

处理：依据用户提供的参考图重做暖色桌面控制台 UI；针对 niri 的 640/960/1280/1920 四档列宽分别使用底部导航、独立图标栏和完整图文侧栏；加入错误/忙碌/禁用状态、缺失客户端引导、快速诊断操作和独立 SVG 应用图标；配置文件改为 `0600`。

### 一般：安装可逆性

旧安装脚本只能安装或升级，无法一致地清理 systemd 服务和用户级安装产物。

处理：增加 `scripts/install.sh --uninstall`，停止并移除服务后清理 GUI、wrapper、官方客户端、桌面入口和图标，同时默认保留用户设置；增加隔离 HOME、XDG 与 systemd 目录的 shell 回归测试。

### 一般：界面代码可维护性

原 `src/ui.rs` 同时包含窗口装配、响应式断点、导航、异步操作、状态同步和通用控件，单文件超过 1600 行。

处理：保持控件树、信号和文案不变，将断点、运行状态、导航和通用组件拆入 `src/ui/` 子模块；重新构建真实 release，并复查连接页与诊断页在 640/960/1280/1920 四档 Niri 宽度下的布局。

### 一般：缺失客户端恢复流程

旧界面在官方客户端缺失时只提示用户把 ZIP 放入 Downloads 并重新运行脚本，无法在应用内恢复。

处理：banner 增加原生 ZIP 文件选择与后台安装；经 root-owned helper 把客户端安装到 `/usr/lib`。helper 从单一文件句柄复制 ZIP 快照，拒绝不安全路径、符号链接和特殊文件，收紧目录/文件权限，验证架构文件，原子替换 wrapper，并在失败时恢复旧客户端。新增有效安装、恶意路径、符号链接源、权限与失败回滚测试，并在隔离环境下实测缺失状态。

### 一般：持续验证

原仓库只有交接文档中的本地验证命令，push 和 pull request 没有自动质量门槛。

处理：增加 Arch Linux 容器 GitHub Actions，自动执行格式、24 项 Rust 测试、Clippy、release 构建、ShellCheck、安装卸载回归、desktop、SVG、polkit policy 和 diff 空白检查。

### 一般：命令行安装事务性

GUI 内安装已具备 ZIP 路径校验和失败回滚，但安装脚本仍会直接删除旧客户端后复制新目录。

处理：脚本先安装 root-owned helper 与 policy，再把 ZIP 交给同一事务安装实现；helper 在私有临时路径准备客户端和 wrapper 后再切换，wrapper 替换失败时恢复旧目录。隔离 shell 测试新增 root-owned 产物、正常安装、不安全 ZIP 和回滚场景。

### 严重：保留授权曾可能执行用户可写程序

若直接对 `~/.local/bin/rjsupplicant` 使用 `auth_admin_keep`，用户可在授权保留期替换程序并获得任意 root 执行，不能通过单纯限制 GUI 参数解决。

处理：新增固定 `/usr/lib/rjsupplicant-gui/rjsupplicant-helper`，只解析六个严格参数数量的白名单动作；官方 wrapper 与客户端同样迁移到 root-owned `/usr/lib`。polkit policy 同时匹配 helper 绝对路径和 `argv1`，仅日常认证/服务动作使用 `auth_admin_keep`，ZIP 安装保持每次 `auth_admin`。停止、禁用或重启前还会拒绝引用用户路径或可写文件的旧 service；旧用户级客户端仅作为无保留授权的手动认证迁移回退。

### 主要：特权安装继承环境与归档文件类型

root helper 若通过 `PATH` 查找解压器、wrapper 继承调用方 `LD_LIBRARY_PATH`，或解压后保留归档中的符号链接和宽松权限，会扩大被替换程序或动态库注入的风险。

处理：helper 安装固定使用 `/usr/bin/unzip`；wrapper 固定使用 `/usr/bin/bash` 和 `/usr/bin/getconf`，只设置客户端自身库目录。ZIP 源以 `O_NOFOLLOW|O_NONBLOCK` 打开，解压前按 Unix 类型拒绝链接/特殊文件，解压后再次递归检查，并规范化为 `0755` 目录及 `0644/0755` 文件。

### 主要：密码曾进入 helper 命令行

通过 `pkexec helper authenticate ... <密码>` 传值会让校园网密码同时出现在 pkexec/helper 与官方客户端的进程参数中，扩大 `/proc/*/cmdline` 的暴露窗口。

处理：GUI 改为通过标准输入向 helper 传送最多 4096 字节的 UTF-8 密码，helper 参数只保留经过校验的 DHCP、网卡、账号和保存开关。终端 sudo 回退会关闭回显后重新提示。官方闭源客户端只接受 `-p`，因此其进程参数中的短暂暴露仍无法消除。

### 一般：首次安装步骤较多

手动安装需要先克隆仓库、寻找学校官方 ZIP，再运行安装脚本，不适合个人机器快速恢复。

处理：增加 GitHub curl bootstrap。它保护已有 Git 修改和分叉，只从项目 GitHub 获取源码；Linux V1.31 客户端只从学校 HTTPS 直链下载，并使用固定 SHA-256 校验后原子放入 `~/Downloads`。已有同名 ZIP 校验不符时直接拒绝，闭源 ZIP 不提交到仓库，客户端已就绪时也不会因重复更新而重装。安装器还会删除最终 release 输出并使用锁定依赖重新链接 GUI/helper，避免信任被替换的缓存二进制。新增完全位于 `/tmp` 的离线回归，不触碰真实安装、网络或服务。

## 保留限制

- 官方客户端是闭源旧程序；`sysctl: 写入错误: 错误的文件描述符` 等上游问题无法由 GUI 内部修复。
- GUI 到 helper 的密码通过标准输入传递，不出现在 pkexec/helper 参数中；官方程序仅提供 `-p`，首次/修改密码时密码仍会短暂存在于官方客户端进程参数中。GUI 自身不落盘保存密码。
- 图形操作依赖 polkit；终端回退依赖本机安装的终端和 sudo 配置。
- 旧用户级客户端路径暂时保留用于升级迁移；只有 root-owned 客户端就绪后才使用新 helper 完整路径。
- policy 只做了离线 XML/DTD 检查，尚未安装到真实系统验证 polkit agent 的授权与取消交互。
- 安装脚本以 Arch Linux 为主要目标，其他发行版只会给出手动依赖提示，未做正式兼容保证。
- 没有官方协议级成功回调，所以 UI 只能可靠说明“认证进程正在运行”；账号或密码是否通过需要看官方日志。

## 验证范围

- 24 项 Rust 单元测试：新旧命令构造、密码标准输入、helper 协议、固定/旧服务文件边界、参数校验与转义、ZIP 安装/权限/拒绝与回滚
- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release`
- `bash -n` 与 ShellCheck
- 隔离环境中的 bootstrap 与安装/卸载回归测试
- desktop 与 SVG、polkit policy XML 校验
- Wayland/niri 下 640px、960px、1280px 和 1920px 四档列宽实图检查

没有在审计期间把 helper/policy 安装到真实系统、主动发起校园网认证或修改系统服务，以免打断当前网络和已有系统状态。
