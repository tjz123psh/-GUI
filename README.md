# 锐捷有线认证

面向广东外语外贸大学校园有线网的 Linux 桌面应用，使用 GTK 4 和 libadwaita 构建，主要适配 Arch Linux 与 Niri。

项目不实现锐捷协议，而是为学校提供的官方 Linux 客户端增加图形界面、状态查看、日志诊断和开机认证管理。

## 主要功能

- 连接、断开校园有线认证
- 自动识别物理有线网卡和网线状态，显示当前网卡 IP/网关
- 查看认证进程、systemd 状态、最近日志预览与连接详情（账号/时长/最近认证结果，实时刷新）
- 管理开机自动认证
- 在应用内安装学校官方客户端 ZIP
- 渐进披露浮层：DHCP/保存密码连接设置与测试连通、重启认证服务、打开实时日志等诊断工具
- 适配 Niri 的窄列、半宽和全宽窗口
- 通过 root-owned helper 和 polkit 安全执行特权操作

界面采用樱花学园主题：整幅樱花插画随窗口宽度在窄列裁切、半景与完整构图间切换，场景层铺满全窗口、标题栏透明沉浸（背景贯穿到顶部，标题与窗口按钮浮于其上）；内容为**舞台 + 控制台**——左侧透明舞台直接透出樱花场景（大状态字 + 设备↔校园网关自绘链路图，认证时链路点亮光点流动 + 4 枚状态胶囊），右侧 420px 单张玻璃控制台（连接表单含网卡 IP/网关、开机自启、连接/断开/安装、最近日志预览、连接详情），窄列自动折叠为单栏实底控制台 + 顶部状态条。配 Tabler 线性玫瑰粉图标，认证成功时画面向暖粉霞光漫开、链路节点泛起光晕。低频工具遵循渐进披露：DHCP、保存密码等连接设置与测试连通、重启开机认证、打开实时日志等诊断动作统一收在标题栏"更多工具"深紫浮层，控制台只保留核心认证流程与实时状态信息。

## 安装

推荐先把源码放到可审查的本地目录，再运行仓库内脚本：

```bash
git clone https://github.com/tjz123psh/-GUI.git ~/.local/src/rjsupplicant-gui
~/.local/src/rjsupplicant-gui/scripts/bootstrap.sh
```

不使用 Git 时，也应先把引导脚本下载为文件、检查后再执行；不要直接使用 `curl | sh`：

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh \
  -o /tmp/rjsupplicant-bootstrap.sh
sed -n '1,240p' /tmp/rjsupplicant-bootstrap.sh
bash /tmp/rjsupplicant-bootstrap.sh
```

安装过程会：

1. 将源码下载或更新到 `~/.local/src/rjsupplicant-gui`；
2. 从广外官网下载并校验 Linux V1.31 客户端 ZIP；
3. 安装 GUI、桌面入口、helper 和 polkit policy。

过程中会请求管理员授权。安装完成后，从应用菜单打开“锐捷有线认证”即可。

如果不希望直接执行网络脚本，可以先查看 [bootstrap.sh](scripts/bootstrap.sh)。

## 更新

重新运行本地 `~/.local/src/rjsupplicant-gui/scripts/bootstrap.sh` 即可。已有官方客户端不会重复安装。

## 卸载

```bash
~/.local/src/rjsupplicant-gui/scripts/bootstrap.sh --uninstall
```

卸载会停止认证服务并删除应用和客户端，但保留账号、网卡等用户偏好。卸载过程会中断当前有线认证。

## 使用说明

1. 打开应用并确认有线网卡；
2. 输入校园网账号和密码；
3. 点击连接；
4. 需要无人值守认证时，再启用“开机自动认证”。

GUI 不保存校园网密码。认证是否最终成功以官方客户端日志为准。

### 安装或重装官方客户端

官方认证客户端不是发行版软件，需要从学校官网下载 Linux 版 ZIP（`RG_Supplicant_For_Linux_V1.31.zip`）后安装：

- 在应用内：点控制台「安装官方客户端」按钮，选择下载好的 ZIP，按提示完成授权安装；
- 若应用检测到已安装的客户端是旧版结构，顶部会弹出横幅提示，「现在处理」按钮进入同一安装流程；
- 命令行方式：`~/.local/src/rjsupplicant-gui/scripts/install.sh`（安装脚本会识别 `~/Downloads` 下的 ZIP，`--help` 可查看其他参数）。

安装会把客户端解压到 `/usr/lib/rjsupplicant/` 并部署 root-owned helper 与 polkit 策略；重装系统后重新运行上述任一入口即可恢复，已有官方客户端不会重复安装。开机自动认证不在安装时创建，需要你在应用里打开「开机自动认证」开关，由提权 helper 生成 `/etc/systemd/system/rjsupplicant.service`。

## 已知安全边界

本项目包装的是学校提供的闭源 Windows 风格 Linux 客户端，有两处限制无法在本仓库内消除，如实说明：

- **口令会出现在进程参数里。** 官方客户端只接受命令行参数传入密码，helper 通过标准输入把口令交给它之后，仍会以参数形式传给该客户端进程。Linux 默认允许任意本地用户读取他人进程的命令行（`/proc/<pid>/cmdline`、`ps`），因此**在同机存在其他用户时，认证期间到会话结束前的口令是可被读到的**。若你的机器有多用户，建议给 `/proc` 挂上 `hidepid=2`（例如 `/etc/fstab` 中 `proc /proc proc defaults,hidepid=2 0 0` 后重启），这样非 root 只能看到自己的进程。
- **客户端会在他用户可读的目录里写下自己的配置。** 官方客户端把设置写在它的工作目录（root 安装时为 `/usr/lib/rjsupplicant/<架构>/`），并以常规权限创建这些文件。若你打开了「保存密码」，请记住凭据由该闭源程序自行保存，本应用不参与其落盘方式；单用户桌面上这不构成实际风险，多用户机器则应结合上一条一起处理。

「保存密码」默认开启，因为开机自动认证的 systemd 单元里没有口令、只能复用客户端记住的密码；关掉它就无法启用开机认证（应用会在你尝试时明确提示）。

## 当前状态

2026-09-01 已在校园有线网完成实机联调：手动认证成功（官方客户端日志「认证成功」+ 中文欢迎横幅 + 网卡拿到 IP），并修复了点击连接卡死、连接后无线网络被官方客户端连带停掉、认证成功但获取不到 IP 三个问题（详情见 [CHANGELOG.md](CHANGELOG.md)）。错误密码、关机自启重启与 polkit 取消/保留授权仍需用户在实机作最终确认。

## 更多文档

- [更新记录](CHANGELOG.md)
- [安全与实现审计](AUDIT.md)
- [开发交接文档](HANDOFF.md)
