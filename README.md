# 锐捷有线认证

面向广东外语外贸大学校园有线网的 Linux 桌面应用，使用 GTK 4 和 libadwaita 构建，主要适配 Arch Linux 与 Niri。

项目不实现锐捷协议，而是为学校提供的官方 Linux 客户端增加图形界面、状态查看、日志诊断和开机认证管理。

## 主要功能

- 连接、断开校园有线认证
- 自动识别物理有线网卡和网线状态
- 查看认证进程、systemd 状态及客户端日志
- 管理开机自动认证
- 在应用内安装学校官方客户端 ZIP
- 适配 Niri 的窄列、半宽和全宽窗口
- 通过 root-owned helper 和 polkit 安全执行特权操作

## 一键安装

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh | bash
```

安装过程会：

1. 将源码下载或更新到 `~/.local/src/rjsupplicant-gui`；
2. 从广外官网下载并校验 Linux V1.31 客户端 ZIP；
3. 安装 GUI、桌面入口、helper 和 polkit policy。

过程中会请求管理员授权。安装完成后，从应用菜单打开“锐捷有线认证”即可。

如果不希望直接执行网络脚本，可以先查看 [bootstrap.sh](scripts/bootstrap.sh)。

## 更新

重新运行同一条安装命令即可。已有官方客户端不会重复安装。

## 卸载

```bash
curl -fsSL https://raw.githubusercontent.com/tjz123psh/-GUI/main/scripts/bootstrap.sh | bash -s -- --uninstall
```

卸载会停止认证服务并删除应用和客户端，但保留账号、网卡等用户偏好。卸载过程会中断当前有线认证。

## 使用说明

1. 打开应用并确认有线网卡；
2. 输入校园网账号和密码；
3. 点击连接；
4. 需要无人值守认证时，再启用“开机自动认证”。

GUI 不保存校园网密码。认证是否最终成功以官方客户端日志为准。

## 当前状态

代码、安装器和离线回归均已完成，但真实校园网连接、错误密码、重启和 polkit 交互仍等待实机验证。

## 更多文档

- [更新记录](CHANGELOG.md)
- [安全与实现审计](AUDIT.md)
- [开发交接文档](HANDOFF.md)
