# 场景素材清单

`src/scene.rs` 通过 `include_bytes!` 编译期嵌入以下素材，运行时不读取文件。

| 文件 | 来源 | 许可 | 说明 |
|------|------|------|------|
| `scene-sakura.png` | 用户本地壁纸《哲风壁纸》樱花（1920x1080） | 用户自有壁纸，仅本地使用 | 明亮粉紫二次元插画：右侧婚纱少女、樱花垂枝、左侧虚化空区。应用内按 1280x720 LANCZOS 压缩后嵌入 |

# 场景素材清单

`src/scene.rs` 通过 `include_bytes!` 编译期嵌入以下素材，运行时不读取文件。

| 文件 | 来源 | 许可 | 说明 |
|------|------|------|------|
| `scene-sakura.png` | 用户本地壁纸《哲风壁纸》樱花（1920x1080） | 用户自有壁纸，仅本地使用 | 明亮粉紫二次元插画：右侧婚纱少女、樱花垂枝、左侧虚化空区。应用内按 1280x720 LANCZOS 压缩后嵌入 |

## data/icons（Tabler 线性图标，svg 源 + 64px RGBA PNG）

- 来源：Tabler Icons（tabler.io），`@tabler/icons` v2.47.0（jsdelivr CDN）。
- 处理：24px SVG 源，`stroke="#b8507c"` 深樱紫描边、stroke-width 2，`rsvg-convert -w 64 -h 64` 渲染 PNG。
- 用途：`src/ui.rs`（icon_image()）与 `src/scene.rs` 链路节点（device-laptop=设备、server=校园网关）。

历史素材（已删除）：`scene-night.png`（Pixabay anime-girl-stars-9063542，深靛蓝夜空水手服少女）与 `scene-dawn.png`（Pixabay sunset-anime-7628294，暖橙落日），均属 Pixabay Content License（可商用免署名），2026-08 因主题转向樱花而移除。
