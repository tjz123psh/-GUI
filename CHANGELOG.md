# Changelog

## 0.3.0 - 2026-07-15

### Added

- 在官方客户端缺失时，通过原生文件选择器安装学校提供的 Linux ZIP。
- `scripts/install.sh --uninstall`，停止服务并清理安装产物，同时保留用户设置。
- Arch Linux GitHub Actions，自动执行 Rust、Shell、desktop、XML 和安装卸载回归检查。
- ZIP 安装、wrapper 生成、失败回滚和不安全路径的 Rust/脚本测试。
- 固定在 `/usr/lib/rjsupplicant-gui` 的 root-owned helper，以及按六个白名单子命令匹配的 polkit policy。

### Changed

- 将 GTK 断点、导航、运行状态和通用组件拆分到 `src/ui/` 子模块。
- GUI 与命令行安装均先准备新客户端和 wrapper，再替换旧安装。
- 官方客户端与 wrapper 迁移到 root-owned `/usr/lib`；旧用户级路径仅作为升级回退。
- 开机认证 service 由 helper 按当前设置生成，安装脚本不再创建参数不完整的默认服务。
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
- 新客户端或 wrapper 安装失败时恢复旧客户端目录。

## 0.2.0 - 2026-07-14

- 重做面向 Niri 四档列宽的暖色桌面界面。
- 修复官方客户端 fork 后 systemd `Type=simple` 立即断开的问题。
- 分离客户端、认证进程、有线链路和开机认证状态语义。
- 将授权、systemctl、日志和状态读取移出 GTK 主线程。
- 增加账号/网卡校验、systemd 参数转义和 `0600` 配置权限。
