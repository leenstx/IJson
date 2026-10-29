# IJson

<div align="center">

A modern, lightweight, and high-performance JSON viewer, editor, and diff tool built with Rust and egui.

一款基于 Rust 和 egui 开发的轻量、高效、现代化的 JSON 可视化查看、编辑与对比工具。

[English](#english) | [简体中文](#简体中文)

</div>

---

<a name="english"></a>
## English

### Overview

**IJson** is a native cross-platform desktop application designed for developers. It offers a split-pane layout with a code editor on the left and an interactive JSON tree view on the right, providing smooth performance even with large payloads.

### Features

- ⚡ **Dual View Layout**: Split-pane view with a draggable divider.
  - **Left**: Raw JSON editor with syntax highlighting, undo/redo, and line numbers.
  - **Right**: Interactive tree view displaying node keys, types, values, and array lengths.
- 🛠️ **Format & Minify**:
  - **Format JSON**: One-click pretty print.
  - **Sort Keys**: Original order, Ascending (A→Z), or Descending (Z→A).
  - **Minify**: Compress JSON into a single line and copy directly to the clipboard.
  - **Minimal JSON & Clear Values**:
    - *1st click*: Extract sample structure (keep only the first element of arrays).
    - *2nd click*: Clear values (numbers → `0`, booleans → `false`, strings → `""`).
- 🌐 **cURL Request Support**:
  - Paste any `curl` command into the editor to automatically execute the request and display the JSON response.
- 🔍 **JSON Comparison (Diff)**:
  - Side-by-side comparison in a dedicated tab.
  - Synchronized scrolling (`Cmd` / `Ctrl` + Scroll).
  - Semantic and text diff highlighting (identifies identical content, formatting differences, and text diffs).
- 🌲 **Tree Node Operations**:
  - In-tree search with match highlighting and prev/next navigation.
  - Expand/collapse all or specific subtrees.
  - Copy Key, Value, JSON Path, or formatted JSON node.
  - In-place node editing (String, Number, Bool, Null) synchronized back to the editor.
  - Node deletion.
  - **Image Preview**: Automatic preview for image URLs and Base64 encoded images with right-click download support.
- 📑 **Tab Management**:
  - Multi-tab support.
  - Pin, rename, and close tabs.
- 🎨 **Appearance & Fonts**:
  - Light, Dark, and Follow System themes.
  - System font scanner and selector with live preview.
  - Window size and state caching across launches.
- 🌐 **Internationalization**:
  - One-click toggle between English and Simplified Chinese (简体中文).

### Keyboard Shortcuts

| Shortcut | Description |
| --- | --- |
| `⌘Z` / `Ctrl+Z` | Undo in editor |
| `⌘⇧Z` / `Ctrl+Shift+Z` | Redo in editor |
| `⌘ + Scroll` / `Ctrl + Scroll` | Synchronized scrolling in comparison view |
| `⌘Enter` / `Enter` | Save node edit |
| `Esc` | Cancel node edit / close modal |

### Build & Run

Ensure you have the Rust toolchain installed ([rustup.rs](https://rustup.rs)).

```bash
# Clone the repository
git clone https://github.com/leenstx/IJson.git
cd IJson

# Run in debug / release mode
cargo run --release

# Build release binary
cargo build --release
```

#### macOS App Bundle

To package a standalone `.app` bundle on macOS:

```bash
cargo install cargo-bundle
cargo bundle --release
```
The generated application will be located in `target/release/bundle/osx/IJson.app`.

### License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

---

<a name="简体中文"></a>
## 简体中文

### 简介

**IJson** 是一款专为开发者打造的轻量级、高性能跨平台 JSON 查看、编辑与比对桌面工具。采用 Rust 配合 egui 编写，拥有极速的启动和渲染性能，并针对 macOS 提供无边框原生视觉体验。

### 核心特性

- ⚡ **双栏对照视图**：支持拖拽调整左右分栏比例。
  - **左侧**：原生文本编辑区，支持语法高亮、撤销/重做、行号显示。
  - **右侧**：交互式 JSON 树形视图，清晰呈现键名、数据类型、值以及数组大小。
- 🛠️ **格式化与压缩**：
  - **格式化 JSON**：一键美化排版。
  - **键名排序**：支持原始顺序、升序（A→Z）与降序（Z→A）切换。
  - **压缩单行**：一键将 JSON 压缩为单行并自动复制到剪贴板。
  - **最小 JSON / 清空值**：
    - *第 1 次点击*：提取最小骨架（数组只保留首个元素）。
    - *第 2 次点击*：清空数据值（数字 → `0`，布尔值 → `false`，字符串 → `""`）。
- 🌐 **cURL 命令直接解析**：
  - 直接在编辑区粘贴 `curl` 命令，自动发起网络请求并加载返回的 JSON 数据。
- 🔍 **JSON 差异比对（Diff）**：
  - 在新标签页中左右双栏比对 JSON。
  - 同步滚动支持（按住 `Cmd` / `Ctrl` 滚动滚轮同步滚动）。
  - 精准识别完全一致、仅排版/键序不同、文本差异三种状态。
- 🌲 **树形节点操作**：
  - 节点搜索与匹配高亮，支持上一个/下一个快捷定位。
  - 全部展开/折叠、展开/折叠当前子树。
  - 复制键名、复制值、复制 JSONPath 路径、复制完整节点 JSON。
  - 原地编辑节点值（支持 String、Number、Bool、Null）并即时回写到编辑区。
  - 节点删除。
  - **图片预览**：自动识别图片链接与 Base64 图片并提供预览，右键可直接保存下载。
- 📑 **多标签页管理**：
  - 支持多标签页并发使用。
  - 支持标签页置顶（Pin）、重命名及关闭。
- 🎨 **外观与字体**：
  - 日间模式（Light）、夜间模式（Dark）与跟随系统。
  - 支持系统字体扫描、搜索与实时预览切换。
  - 自动保存并恢复窗口位置与尺寸。
- 🌐 **双语支持（i18n）**：
  - 内置中文与英文语言界面，一键无缝切换。

### 快捷键

| 快捷键 | 功能说明 |
| --- | --- |
| `⌘Z` / `Ctrl+Z` | 撤销编辑 |
| `⌘⇧Z` / `Ctrl+Shift+Z` | 重做编辑 |
| `⌘ + 滚轮` / `Ctrl + 滚轮` | 比对模式下双栏同步滚动 |
| `⌘Enter` / `Enter` | 保存节点编辑 |
| `Esc` | 取消节点编辑 / 退出弹窗 |

### 编译与运行

请确保本地已安装 Rust 环境 ([rustup.rs](https://rustup.rs))。

```bash
# 克隆仓库
git clone https://github.com/leenstx/IJson.git
cd IJson

# 直接运行（推荐 release 模式获得最佳性能）
cargo run --release

# 编译 release 可执行文件
cargo build --release
```

#### macOS 打包应用

在 macOS 上打包独立 `.app` 应用程序：

```bash
cargo install cargo-bundle
cargo bundle --release
```
打包产物位于 `target/release/bundle/osx/IJson.app`。

### 开源协议

本项目基于 [GPL-3.0 License](LICENSE) 开源。
