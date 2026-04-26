# wisp · 项目设计定稿 v2

## 一、项目定位

`wisp` 是一个面向 Linux 的现代化磁盘清理与分析 CLI 工具，兼具命令式清理和交互式 TUI 分析两种形态。目标用户一次输入即能完成常见清理，高级用户可通过 TUI 深入分析磁盘占用并交互式删除。设计上预留多发行版、多清理源扩展能力。

名字取"一缕、一阵轻风"之意，暗喻把杂物轻扫而去。二进制名 `wisp`，crate 前缀 `wisp-*`，配置目录 `~/.config/wisp/`。

## 二、核心设计原则

### 2.1 分层架构（硬约束）

自底向上五层，**严禁反向依赖或跨层跳跃**：

**L1 · Platform**：屏蔽发行版差异。`Distro` / `PackageManager` / `InitSystem` trait。Arch 先实现，接口按通用性设计。发行版检测（`/etc/os-release`）在这层。

**L2 · FS Core**：目录遍历、大小统计、元数据、回收站抽象、权限检测、黑名单校验、路径规范化。对上层只暴露"安全的文件系统操作"。**不关心清理什么，只关心怎么安全地扫和删**。

**L3 · Cleaners**：每个清理目标是一个 `Cleaner` trait 实现，独立、无状态、可组合。分 `system` / `user` / `dev` 三个子域。**只声明"要清理什么"，不触碰文件系统**，实际操作下放给 L2。

**L4 · Engine**：把 L3 规则按用户意图组装成 `CleanPlan`，处理依赖、风险评估、dry-run、并发执行、进度事件流、历史写入、确认回调。**所有批处理逻辑都在这里**。

**L5 · Presentation**：三种形态共享 Engine 抽象：
- **CLI**（clap）：直接输出或流式 JSONL
- **Prompt**（inquire）：命令模式下的确认交互
- **TUI**（ratatui）：全屏交互

**L5 绝不直接调用 L2/L3**，必须通过 Engine。

横切关注点独立模块：`config`、`logging`（tracing）、`errors`、`i18n`（预留）、`telemetry`（本地统计，无上报）。

### 2.2 执行模型统一

**Engine 只认 tokio**。扫描虽用 rayon，但包装在 `tokio::task::spawn_blocking` 里对外呈现为 `Future`。L5 与 L4 交互永远通过 async + channel，不存在 rayon 和 tokio 混合的调用点。这样 L5 的三种形态只需要处理一种并发模型。

### 2.3 Cleaner 接口的双态拆分

为绕开 `async fn in trait` 不能做 `dyn` 的限制，`Cleaner` 拆两半：

- **`CleanerMeta`（同步，可 dyn）**：返回 id、名称、描述、风险等级、所需权限、预估大小、平台适用性。这部分做成 trait object 用于列出、过滤、展示。
- **`CleanerExec`（异步，通过 Engine 调用）**：`async fn plan(&self, ctx: &Ctx) -> Result<Vec<CleanAction>>`。不做成 dyn，而是 Engine 内部枚举 dispatch，或用 `enum CleanerKind` 编译期穷举。

这样既有动态性（展示层）又有高效 async（执行层），且 agent 能很清楚每加一个 Cleaner 要改两个地方。

### 2.4 编译期注册

用 `linkme` crate 实现 Cleaner 编译期自动收集。每个 Cleaner 模块文件末尾：

```rust
#[linkme::distributed_slice(CLEANERS)]
static THIS: CleanerEntry = CleanerEntry { ... };
```

Engine 启动时 `CLEANERS.iter()` 即得全表。**新增 Cleaner 零改动注册表**，符合前面"agent 不容易忘"的目标。

## 三、工作区结构

```
wisp/
├── Cargo.toml                    # workspace 根（含 [workspace.package/dependencies/lints]）
├── rust-toolchain.toml
├── deny.toml
├── crates/
│   ├── wisp-core/                # L2 + 横切类型，零 UI 依赖
│   ├── wisp-platform/            # L1
│   ├── wisp-cleaners/            # L3
│   ├── wisp-engine/              # L4
│   ├── wisp-tui/                 # L5 TUI
│   └── wisp-cli/                 # L5 CLI + 二进制入口（唯一 bin）
├── xtask/                        # 构建辅助（man、completion、打包）
└── docs/
    ├── architecture.md
    ├── contributing.md
    └── adding-a-cleaner.md
```

workspace 根 `Cargo.toml` 使用 `[workspace.package]` 统一元数据，子 crate 用 `version.workspace = true`、`edition.workspace = true`、`license.workspace = true`、`repository.workspace = true`、`rust-version.workspace = true` 继承。依赖版本同样集中在 `[workspace.dependencies]`，子 crate 写 `foo.workspace = true`。

## 四、命令层级设计

采用 **"名词 + 动词"** 子命令结构。顶级命令域尽量精简，相关功能合并。

### 4.1 顶层结构

```
wisp [全局选项] <command> [子命令] [参数]
```

**全局选项**：
- `-v / -vv / -vvv`：日志等级
- `-q / --quiet`：静默
- `-y / --yes`：自动确认中等风险操作（高风险仍需显式确认）
- `-n / --dry-run`：预演不执行
- `--no-trash`：跳过回收站直接删除
- `--no-color`
- `--output <fmt>`：`human`（默认）/ `json`（一次性） / `jsonl`（流式事件）
- `--config <path>`
- `--profile <name>`

### 4.2 命令域（相比 v1 精简）

**`wisp`（无参数）** → 进入 TUI 主菜单，等价 `wisp tui`。

**`wisp tui [page]`** → 显式 TUI，可选 `analyze` / `clean` / `history` 直达页面。

**`wisp clean <target|@group>`** → 命令式清理。核心命令。
- 目标：`pacman` / `aur` / `journal` / `tmp` / `orphans` / `browser` / `thumbnails` / `trash` / `cargo` / `npm` / `pip` / `go` / `docker` / `flatpak` / ...
- 组：`@system` / `@user` / `@dev` / `@all`
- 子命令：
  - `wisp clean list [--group <g>] [--risk <level>]`：列出可用 target
  - `wisp clean info <target>`：详情（动什么、要不要 sudo、预估大小）

**`wisp analyze [path]`** → 磁盘分析。**v1 的 `scan` 合并进来**，通过选项区分：
- `--top N` / `--depth N` / `--min-size <size>`：过滤
- `--cache` / `--use-cache`：保存/复用扫描结果（替代独立的 `scan` 命令）
- `--format <treemap|tree|flat>`：可视化形态
- 子命令：
  - `wisp analyze cache list`：列出已缓存扫描
  - `wisp analyze cache clear`

**`wisp history`** → 删除历史管理。
- `list [--since 7d] [--limit N]`
- `show <id>`
- `restore <id>`（仅回收站删除可恢复）
- `clear`

**`wisp state`** → 统一管理用户状态数据。**v1 的 `favorites` 合并进来**，未来"常用规则组"、"自定义 Cleaner"也归这里。
- `state fav add/list/remove`（原 favorites）
- `state export <path>` / `state import <path>`：迁移或备份整个状态目录

**`wisp config`** → 配置管理。
- `path` / `edit` / `show [key]` / `set <key> <value>` / `reset`

**`wisp profile`** → 命名配置档案。
- 语义明确：**profile = 一组 cleaner 选择 + 一组默认选项覆盖**。例如 "conservative" profile 只选 `@user` 且默认 `--dry-run`。
- `list` / `create` / `delete` / `show <name>` / `use <name>`（设为默认）

**`wisp doctor`** → 环境自检。检查发行版支持、外部工具、权限、挂载、回收站可用性。排查第一道命令。

**`wisp completion <shell>`** → 生成 bash/zsh/fish/elvish/nu 补全。

**`wisp man`** → 生成 man page。

### 4.3 命令扩展规则（写入 CONTRIBUTING）

1. **新功能必须归入现有命令域**，不允许轻率新加顶级命令；新增顶级命令需 RFC。
2. **动词统一词汇表**：`list` / `show` / `add` / `remove` / `clear` / `info` / `edit` / `set` / `get` / `reset` / `run` / `use` / `import` / `export`。禁止同义替换（不准 `display`/`delete`/`find`）。
3. **每个命令的 `--help`** 走统一 clap template：一行摘要 + DESCRIPTION + EXAMPLES + SEE ALSO。
4. **输出格式统一**：
   - `human`：人类可读
   - `json`：一次性 `OutputEnvelope<T>`（含 `version`、`command`、`data`、`warnings`、`errors`）
   - `jsonl`：流式，每行一个 `ProgressEvent` 或最终 `ResultEvent`
5. **退出码统一**（对齐 sysexits.h）：`0` 成功 / `1` 通用错误 / `2` 用户取消 / `3` 权限不足 / `4` 部分失败 / `64` usage 错误 / `70+` 内部错误。
6. **参数命名**：路径叫 `path`，id 叫 `id`，目标叫 `target`，数量叫 `limit`，大小叫 `size`。禁止同义替换。

## 五、技术栈（Rust 1.90+ / 2024 edition）

**工具链**：Rust **1.90+**（2024 edition），MSRV 锁 `1.90`。本地可用更新版本（如 1.94.1），但 CI 用 `1.90` 作为 MSRV gate，同时跑 `stable` 和 `beta`。

`rust-toolchain.toml`：

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy", "rust-src"]
profile = "default"
```

MSRV 通过 `Cargo.toml` 的 `rust-version = "1.90"` 和 CI 矩阵保证，不在 toolchain 文件钉死具体版本。

### 5.1 语言特性利用

- **2024 edition**：新 `unsafe` 语义、`let`/`match` 临时作用域收紧、`gen` 块、`impl Trait` 捕获规则
- **`async fn in trait` / RPITIT**（1.75+）：engine 层 Cleaner 执行接口直接 async，不用 `async-trait` 宏
- **`let chains`**（1.88）：扫描器和路径校验里大量使用
- **`LazyLock` / `OnceLock`**（1.80）：黑名单、编译期正则，砍掉 `once_cell`
- **`Result::flatten` / `Option::is_none_or`**（1.82）
- **`#[expect(...)]`**（1.81）：替代部分 `#[allow]`，lint 消失会报错，强化卫生
- **async closures**（1.85）：engine 进度流回调更自然
- **trait upcasting**（1.86）：`&dyn CleanerMeta` 能顺利 upcast 到基础 trait
- **resolver = "3"**：workspace 默认

### 5.2 核心 crate

| 用途 | crate | 版本下限 |
|---|---|---|
| CLI 解析 | `clap` (derive + cargo) | 4.5 |
| TUI | `ratatui` | 0.29 |
| 终端后端 | `crossterm` | 0.28 |
| 交互 prompt | `inquire` | 0.7 |
| 并行遍历 | `jwalk` | 0.8 |
| 异步运行时 | `tokio` (rt-multi-thread, fs, process, signal, sync) | 1.40 |
| 进度 | `indicatif` | 0.17 |
| 错误（库） | `thiserror` | 2.0 |
| 错误（bin） | `color-eyre` | 0.6 |
| 日志 | `tracing` + `tracing-subscriber` + `tracing-appender` + `tracing-error` | 0.1 / 0.3 / 0.2 / 0.2 |
| 序列化 | `serde` / `serde_json` / `toml` | 1.0 / 1.0 / 0.8 |
| 路径 | `camino` + `directories` | 1.1 / 5.0 |
| 回收站 | `trash` | 5.2 |
| 人类可读 | `humansize` | 2.1 |
| 数据并行 | `rayon` | 1.10 |
| 小集合 | `smallvec` | 1.13 |
| 紧凑字符串 | `compact_str` | 0.8 |
| 索引池 | `slotmap` | 1.0 |
| 全局分配器 | `mimalloc` | 0.1 |
| 编译期注册 | `linkme` | 0.3 |
| 测试 | `insta` / `assert_cmd` / `predicates` / `tempfile` / `proptest` / `rstest` | 最新 |
| 基准 | `criterion` | 0.5 |

### 5.3 依赖卫生

- 根 `[workspace.dependencies]` 统一版本，子 crate 用 `foo.workspace = true`
- 根 `[workspace.lints.rust]` 和 `[workspace.lints.clippy]`，打开 `pedantic` + `nursery`，禁用 `unwrap_used` / `expect_used`（测试除外）
- `cargo-deny` 配 `deny.toml`：禁 copyleft、禁重复依赖、卡 CVE
- `cargo-hakari` 统一 workspace 公共依赖特性
- 禁带 C 依赖的 crate（`mimalloc` 例外，因为它是性能关键且构建稳定）

### 5.4 Profile

```toml
[profile.release]
lto = "fat"
codegen-units = 1
strip = "symbols"
panic = "abort"

[profile.release-small]
inherits = "release"
opt-level = "z"

[profile.dev]
split-debuginfo = "unpacked"   # Linux 下加速 incremental
```

## 六、数据模型核心类型

在 `wisp-core` 定稿，跨层使用，**字段变更视为 breaking**：

```rust
pub enum RiskLevel {
    Trivial,    // 缩略图、浏览器 HTTP 缓存等（几乎绝对安全）
    Safe,       // pacman 缓存、journal 日志等（默认安全，但有极端场景）
    Moderate,   // 孤儿包、flatpak 未用数据（需用户了解影响）
    Dangerous,  // /tmp 清空、docker prune -a 等（必须显式确认）
}

pub struct CleanerId(CompactString);          // 稳定字符串 id，如 "arch.pacman"

pub trait CleanerMeta: Send + Sync {
    fn id(&self) -> CleanerId;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn risk(&self) -> RiskLevel;
    fn requires_root(&self) -> bool;
    fn supported_on(&self, distro: &dyn Distro) -> bool;
    fn group(&self) -> CleanerGroup;          // System / User / Dev
}

pub struct CleanPlan {
    pub id: Uuid,
    pub actions: Vec<CleanAction>,
    pub estimated_size: u64,
    pub required_privileges: Privileges,
    pub risk: RiskLevel,                       // 聚合后最高
}

pub enum CleanAction {
    Delete { path: Utf8PathBuf, size: u64, via: DeletionVia },
    RunExternal { cmd: Command, estimated_size: Option<u64> },
}

pub enum DeletionVia { Trash, Direct }

pub enum ProgressEvent {                       // 流式 JSONL 和 TUI 的共同输入
    PlanBuilt(CleanPlanSummary),
    ActionStarted { id: ActionId },
    ActionProgress { id: ActionId, bytes_done: u64 },
    ActionFinished { id: ActionId, result: ActionResult },
    PlanFinished(CleanReport),
    Warning(String),
}

pub struct CleanReport { /* 逐项成功/失败/跳过 + 总量 */ }

pub struct ScanNode { /* slotmap 索引式目录树 */ }

pub enum Confirmation {                        // L5 → L4 的确认响应
    Approved,
    Denied,
    ApprovedAll,
}

pub trait Confirmer: Send + Sync {             // L4 持有，L5 实现
    async fn ask(&self, req: ConfirmRequest) -> Confirmation;
}

pub struct OutputEnvelope<T> {
    pub version: &'static str,                 // 固定语义化版本
    pub command: String,
    pub data: T,
    pub warnings: Vec<String>,
    pub errors: Vec<ErrorInfo>,
}
```

`OutputEnvelope<T>` 和 `ProgressEvent` 都实现 `Serialize`，`--output json` 走前者，`--output jsonl` 走后者。

## 七、安全模型

### 7.1 硬编码黑名单

`/`、`/home`、`/home/$USER`、`/etc`、`/boot`、`/usr`、`/bin`、`/sbin`、`/lib`、`/lib64`、`/root`、`/proc`、`/sys`、`/dev`、`/run`、`/var`（白名单例外：`/var/cache/pacman/pkg`、`/var/log/journal` 等）。

**所有路径操作前 canonicalize**，防符号链接绕过。**黑名单校验在 L2，所有层向下都无法越过**。

### 7.2 风险分级与确认

- `Trivial`：`-y` 批量跳过，TUI 不弹确认
- `Safe`：`-y` 跳过，TUI 单次确认
- `Moderate`：`-y` 跳过但显示摘要，TUI 需逐项确认
- `Dangerous`：**不受 `-y` 影响**，CLI 需 `--yes-dangerous`，TUI 需输入 `yes` 全拼

### 7.3 默认回收站

用户域删除默认走 `trash` crate。系统域因体积或分区限制走直接删但要求 `Moderate+`。`--no-trash` 可强制直删（审计日志标记）。

### 7.4 dry-run 语义

`-n` 时走同一套代码路径，在 L2 的最终 syscall 处分叉（回收/直删不执行，但所有前置校验、大小统计、Plan 构建照跑）。**保证 dry-run 和实际行为完全一致**，没有"dry-run 通过但实际跑失败"。

### 7.5 审计日志

所有删除操作写 `~/.local/state/wisp/audit.log`（JSONL），字段：时间戳、用户、cleaner id、路径、大小、删除方式、是否 dry-run、plan id。

### 7.6 可观测性（tracing span 结构）

规定 span 层级：

```
wisp.run                  # 整次调用根 span
├── wisp.plan             # Plan 构建
│   └── cleaner.<id>      # 每个 Cleaner 的 plan
├── wisp.confirm          # 确认交互
└── wisp.execute
    └── action.<id>       # 每个 action 执行
        └── fs.<op>       # 底层 FS 操作
```

`tracing-appender` 写到 `~/.local/state/wisp/log/wisp.YYYY-MM-DD.log`，默认 warn，`-v` info，`-vv` debug，`-vvv` trace。`tracing-error` 在错误回溯中附带 span 上下文，出问题时能一眼看到"是哪个 Cleaner 的哪个 action 炸了"。

## 八、TUI 信息架构

### 8.1 主菜单

四块入口：
1. **Recent**：最近 5 条 history
2. **Favorites**：用户收藏的 target 和 path
3. **Quick Clean**：按发行版预设的清理组
4. **Analyze**：选择路径进入 analyzer

### 8.2 分析器

左右双栏：左侧面包屑 + 子目录列表（按大小排序），右侧可视化（默认 treemap，`v` 切到扇形/柱状）。底部状态栏：当前路径、总大小、已选中项大小、快捷键。

### 8.3 交互约定（vi + 常用并存）

- `j/k` / `↑↓` 移动 · `Enter`/`l` 进入 · `h`/`←`/`Backspace` 返回
- `d` 删除（弹确认浮层）· `Space` 多选 · `*`/`f` 收藏
- `r` 重扫 · `v` 切换可视化 · `/` 过滤 · `?` 帮助 · `q`/`Esc` 退出

### 8.4 删除确认浮层

显示：完整绝对路径、文件数、总大小、删除方式（回收/直删）、风险等级。`Dangerous` 需输入 `yes` 全拼。

### 8.5 进度展示

TUI 订阅 Engine 的 `ProgressEvent` channel，渲染为右下角浮动进度区域，每个 action 一行。支持取消（发送取消信号给 Engine，Engine 在下一个可中断点停止）。

## 九、配置与持久化

遵循 XDG：
- 配置：`$XDG_CONFIG_HOME/wisp/config.toml`（默认 `~/.config/wisp/config.toml`）
- Profile：`$XDG_CONFIG_HOME/wisp/profiles/<name>.toml`
- 扫描缓存：`$XDG_CACHE_HOME/wisp/scans/`（默认 `~/.cache/wisp/scans/`）
- 状态（历史、收藏、审计）：`$XDG_STATE_HOME/wisp/`（默认 `~/.local/state/wisp/`）
- 日志：`$XDG_STATE_HOME/wisp/log/`

**配置结构按命令域分 section**（与命令层级一一对应）：

```toml
[general]
default_profile = "default"
color = "auto"
confirm_dangerous = true

[clean]
default_group = "@user"
prefer_trash = true

[analyze]
default_depth = 5
default_format = "treemap"

[tui]
vim_keys = true
theme = "dark"

[cleaners.pacman]
keep_versions = 2
enabled = true

[cleaners.docker]
enabled = false
```

**Profile 语义明确**：Profile 是"cleaner 白名单 + 默认选项覆盖"的组合。例如：

```toml
# profiles/conservative.toml
cleaners = ["browser", "thumbnails", "trash"]    # 白名单
overrides = { "clean.prefer_trash" = true, "general.dry_run_default" = true }
```

## 十、分阶段实施路线

每阶段产出可演示功能 + 单元测试 + 文档，不交叉推进。

**阶段 0 · 脚手架（强制先完成）**  
workspace 布局、MSRV 锁 1.90、`[workspace.package/dependencies/lints]`、CI（fmt/clippy/test/audit/deny/msrv）、CONTRIBUTING（含命令层级和分层规则）、核心类型定义（第六节全部）、错误类型、tracing 初始化、配置加载。验收：`wisp --version` 可跑，`cargo test` 全绿，CI 矩阵全绿。

**阶段 1 · L1 + L2**  
`Distro` trait + Arch 实现、扫描器（jwalk + spawn_blocking 封装为 async）、路径安全校验、黑名单、回收站封装、dry-run 框架。验收：`wisp doctor` 和 `wisp analyze` 可跑。

**阶段 2 · L3 Arch 系统清理器**  
pacman、paccache、orphans、journal、tmp。每个 Cleaner 独立 PR，用 linkme 自动注册。验收：`wisp clean pacman -n` 正确。

**阶段 3 · L3 用户与开发清理器**  
browser、thumbnails、trash、cargo、npm、pip、go、flatpak、docker。验收：`wisp clean @user -n` 聚合正确。

**阶段 4 · L4 编排层**  
Plan 构建、`Confirmer` trait、并发执行、`ProgressEvent` channel、历史记录、审计日志、tracing span 结构。验收：`wisp clean @all -y -n --output jsonl` 流式输出正确，`jq` 可消费。

**阶段 5 · L5 CLI 完整化**  
所有命令域实现、shell completion、man page、三种输出格式、退出码规范。验收：命令层级文档每个命令都能跑。

**阶段 6 · L5 TUI**  
主菜单、analyzer、cleaner 页面、history、favorites、删除确认、进度浮层。treemap 优先。验收：完整流程跑通（扫描 → 进入 → 删除 → 回退 → 收藏）。

**阶段 7 · 可视化增强** ✅  
Canvas 极坐标扇形图，键盘导航扇区。`v` 切换 bars / sectors，状态栏显示当前 viz 模式。

**阶段 8 · 打包与发布** ✅  
AUR PKGBUILD（`wisp` 和 `wisp-git`，`packaging/aur/`）、GitHub Release workflow（tag 触发，三 target 二进制 + completion + man）、crates.io 发布脚手架（`wisp-platform` / `wisp-core` / `wisp-cleaners` 加 publish 元数据，`packaging/README.md` 记录 manual + workflow 流程）、双语 README（英文 + 中文 fold）。asciinema 演示需用户在真实终端录制后挂在 README。

**阶段 9 · 扩展发行版（未来）**  
Debian / Fedora / openSUSE。只加 L1 实现 + 对应 L3 Cleaner，L2/L4/L5 零改动。

## 十一、给 agent 的硬性规约（写入 CONTRIBUTING）

1. **分层约束**：任何 PR 的 `use` 语句违反 L1 → L5 单向依赖直接拒绝。
2. **新命令提案**：新增顶级命令或命令域必须先开 issue 讨论，遵循第四节规范。
3. **新 Cleaner 清单**：
   - 实现 `CleanerMeta`
   - 在 `CleanerKind` enum 增加 variant 并实现执行逻辑
   - `#[linkme::distributed_slice(CLEANERS)]` 自动注册
   - 声明 `RiskLevel`
   - 提供 dry-run 路径
   - 至少一个单元测试 + 一个 proptest
   - 更新 `docs/cleaners.md`
4. **安全测试**：涉及删除的 PR 必须新增 proptest 覆盖路径注入、符号链接、相对路径、UTF-8 边界。
5. **性能回归**：扫描关键路径 PR 必须附 criterion 对比。
6. **依赖引入**：新 crate 依赖需在 PR 描述说明为何 std 或现有依赖不够用，并通过 `cargo-deny` 检查。
7. **日志规范**：所有跨层调用必须开 tracing span，名称遵守第 7.6 节层级。
8. **输出规范**：任何新命令必须同时支持 `human` / `json` / `jsonl` 三种输出（如果输出语义不支持流式，`jsonl` fallback 到 `json`）。




