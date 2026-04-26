<div align="center">

# wisp

**现代化的 Linux 磁盘清理与分析工具**

[![CI](https://github.com/chiiydd/wisp/actions/workflows/ci.yml/badge.svg)](https://github.com/chiiydd/wisp/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](#许可证)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-orange)](rust-toolchain.toml)

CLI + TUI 一体的磁盘清理工具：清 `pacman` 缓存、journal、孤儿包、浏览器缓存、回收站、开发缓存（cargo / npm / pip / go / docker），并提供带极坐标扇形图的交互式目录分析器。

[English](README_EN.md) · 中文

</div>

---

## 特性

- **一键清理**：`wisp clean @user / @system / @dev / @all`，默认 dry-run，安全可预览。
- **分目标清理器**：pacman、paccache、orphans、journal、/tmp、回收站、浏览器缓存、缩略图、cargo、npm、pip、go、flatpak、docker。
- **交互 TUI**：neovim 风格 chrome，模式徽章、状态栏键位提示；按 `v` 在柱状图与极坐标扇形图之间切换。
- **默认安全**：三档风险（Safe / Moderate / Dangerous），显式确认，硬编码路径黑名单，删除默认走回收站，`--purge` 才永久删除。
- **流式输出**：`--output json|jsonl` 适配脚本，`human` 适配终端阅读。
- **审计与回退**：每次删除都记录大小、目标与时间戳，`wisp history list` / `restore` 可查可回退。
- **shell 补全 + man page**：`wisp completion zsh`、`wisp man` 一键生成。

## 安装

### AUR（Arch 系推荐）

```sh
# 稳定版
paru -S wisp
# 或 yay -S wisp

# 跟随 master 的开发版
paru -S wisp-git
```

### 从源码构建

需要 Rust 1.90+。

```sh
git clone https://github.com/chiiydd/wisp
cd wisp
cargo install --path crates/wisp-cli --locked
```

二进制安装在 `~/.cargo/bin/wisp`。

### 预编译二进制

在 [Releases 页面](https://github.com/chiiydd/wisp/releases) 下载对应 target 的 tarball，把 `wisp` 放进 `$PATH` 即可。

## 快速上手

```sh
wisp                       # 进入 TUI
wisp clean @user -n        # 预览用户态清理（不真删）
wisp clean pacman -y       # 真正执行 pacman 缓存清理
wisp analyze ~/Downloads   # 一次性分析目录（不进 TUI）
wisp doctor                # 环境与权限检查
wisp history list          # 查看历史清理记录
```

## 命令总览

```
wisp [--output human|json|jsonl]
  ├─ tui                    # 全屏交互界面
  ├─ clean <target> [-n|-y] # @all · @system · @user · @dev · 或单清理器
  │   ├─ list               # 列出可用清理器
  │   └─ info <target>      # 查看单个清理器详情
  ├─ analyze [path]         # treemap / tree / flat 三种视图
  │   └─ cache list|clear   # 管理已保存的扫描结果
  ├─ history list|show|restore|clear
  ├─ state fav add|list|remove · path · export · import
  ├─ config show|edit|set|reset
  ├─ profile list           # 命名清理 profile
  ├─ doctor                 # 环境诊断
  ├─ completion <shell>     # bash · zsh · fish · powershell · elvish
  └─ man                    # 生成 man page
```

## TUI 按键

| 按键           | 行为                                  |
| -------------- | ------------------------------------- |
| `j` / `k`      | 向下 / 向上移动                       |
| `h` / `l`      | 返回 / 进入                           |
| `⏎`            | 激活当前选中项                        |
| `v`            | 切换柱状图 ↔ 极坐标扇形图              |
| `space`        | 标记待删（analyzer）                  |
| `d`            | 删除已标记项                          |
| `q` / `Esc`    | 退出 / 返回上一页                     |

状态栏会同步显示当前模式、上下文（路径 / 计数 / 可视化模式）以及当前页面对应的键位提示。

## 架构

五层 workspace，自下而上严格单向依赖（L1 → L5）：

| 层 | crate            | 职责                                                                                       |
| -- | ---------------- | ------------------------------------------------------------------------------------------ |
| L1 | `wisp-platform`  | Distro / 包管理器 / init system trait 抽象。当前实现 Arch，结构上为多发行版预留扩展点。   |
| L2 | `wisp-core`      | 文件系统扫描、大小统计、黑名单、回收站、路径安全。**只关心如何安全地扫和删，不关心清理什么**。 |
| L3 | `wisp-cleaners`  | 每个清理器声明"清理什么"，绝不直接动文件系统。借 `linkme` 编译期自动注册。                 |
| L4 | `wisp-engine`    | 把清理器组装成 `CleanPlan`，调度执行、产出 `ProgressEvent` 事件流、写历史与审计。           |
| L5 | `wisp-cli` + `wisp-tui` | 表现层（clap / inquire / ratatui），只通过 Engine 与底层交互。                       |

更多细节见 [docs/architecture.md](docs/architecture.md) 与 [docs/adding-a-cleaner.md](docs/adding-a-cleaner.md)。

## 安全模型

- 硬编码黑名单（`/`、`/etc`、`/usr`、`/home`、`/var`…）在到达执行器前直接拒绝。
- 三档风险等级；**Dangerous** 级别的清理器即便确认过仍要求显式 `--yes`。
- 删除默认走系统回收站；如需永久删除请加 `--purge`。
- `clean` 默认是 `--dry-run`（别名 `-n`）；只有显式 `-y` / `--yes` 才会真正动文件。
- 审计日志写入 `~/.local/state/wisp/history.jsonl`。

## 开发

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test  --all-targets --all-features
cargo deny check
```

CI 在每次 push / PR 上跑：fmt / clippy / test（stable / beta / MSRV 1.90）/ cargo-deny。

贡献指南、分层契约、新增命令与新增 Cleaner 的规范见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

[MIT License](LICENSE) · 详见 <https://opensource.org/licenses/MIT>
