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

安装会把客户端解压到 `/usr/lib/rjsupplicant/` 并配置提权与开机认证所需文件；重装系统后重新运行上述任一入口即可恢复，已有官方客户端不会重复安装。

## 当前状态

代码、安装器和离线回归均已完成，但真实校园网连接、错误密码、重启和 polkit 交互仍等待实机验证。

## 更多文档

- [更新记录](CHANGELOG.md)
- [安全与实现审计](AUDIT.md)
- [开发交接文档](HANDOFF.md)
