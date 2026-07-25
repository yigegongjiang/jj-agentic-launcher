```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

## [1.0.1] - 2026-07-25

### Changed

- `--loop auto` 改名 `--loop relay`, 不保留别名 (旧写法直接报错)。`auto` 描述的是"次数自动", 与 `refine` 不在同一语义维度, 且掩盖了 refine 同样由 agent 自决停止; `relay` 直指"接力传 baton", 与 `refine` (零上下文重做) 成对。
  - `LoopSpec::Auto` → `Relay`, `AutoLoopState` → `LoopState`, `DEFAULT_AUTO_MAX_ITER` → `DEFAULT_MAX_ITER`, `build_protocol_prompt` → `build_relay_protocol_prompt`; system-prompt 协议头 `[JJ_LOOP_AUTO]` → `[JJ_LOOP_RELAY]`。
  - 无兼容分支: `parse_flags` 只认 `"relay"` / `"refine"` / 正整数, `--loop auto` 落入 Usage 错误路径。
  - 顺带修 `build_help_text` Usage 块列对齐: 描述列统一到运行时第 54 列 (续行原硬编码 42 空格, 未计入 `{NAME}` 展开的 +12 偏移)。
- `/handoff` 端点 `mode` 字段值同步 `auto` → `relay`。
  - `run_agent_loop` 的 `mode: &'static str` 实参改为 `"relay"`, 经 `server::start` 透传进 `Snapshot.mode`。

## [1.0.0] - 2026-07-23

### Changed

- 运行时从 Bun/TypeScript 迁移到 Rust: 单文件二进制体积 ~50MB → ~0.5MB, 启动更快。命令行 / scene / config / loop / handoff 行为完全不变, `~/.config/jj-prompt-launcher/` 无需迁移。
  - `src/*.ts` + `build.ts` + `package.json` + `bun.lock` + `tsconfig.json` 全删, 换 `Cargo.toml` + `src/*.rs`; crate 依赖仅 `serde` / `serde_json` (preserve_order) / `sha2`。
  - scenes 用 `include_str!` 编译期嵌入; VERSION 用 `env!(CARGO_PKG_VERSION)` 注入; observation server 从 `Bun.serve` 换 `std::net::TcpListener` 手写 HTTP/1.1 + detach 线程; ISO8601 时间戳手写 (civil-from-days), 无 date crate。
  - stream 按 `\n` 字节切行 + 整行 decode; print 累积原始字节到 `parse_handoff` 才 decode — 规避 CJK 跨 chunk 边界。截断 char-safe (`chars().take(n)`)。
  - `.github/workflows/release.yml` 改 cargo 双 target (arm64 原生 + x86_64 交叉), 产物 rename 为 `jj-prompt-launcher-darwin-{arm64,x64}` (asset 名契约不变); checksums 用 `shasum -a 256`。
- `update` 子命令下载改用系统 `curl` (与 install.sh 一致), 进度条为 curl 原生样式; macOS 自带 `curl`, 无需额外安装。
  - `src/update.rs` `curl -fL --progress-bar` 下载 + `curl -fsSL` 取 checksums; sha256 校验用 `sha2` crate; 原子替换 tmp+rename 逻辑不变。

## [0.12.0] - 2026-07-23

### Changed

- **Breaking**: 全项目改名 `jjlauncher` / `cli-prompt-launcher` → `jj-prompt-launcher` (命令名 + `package.json#name` + binary 产物名 + repo 名统一)。
  - `package.json#name` / `build.ts` BUILD_NAME / `install.sh` asset 名 / `.github/workflows/release.yml` dist glob 全部同步。
- **Breaking**: config 目录 `~/.config/cli-prompt-launcher/` → `~/.config/jj-prompt-launcher/`, 旧目录需手动 `mv` 迁移。
  - `src/config.ts` `getConfigDir()` 硬编码路径改名, 无自动迁移逻辑。

## [0.11.1] - 2026-06-09

### Fixed

- `--mcp-config=<path>` 等号连接格式现在正确执行文件存在性检查, 与空格分隔格式行为一致。
  - `src/run.ts` mcp-config 过滤支持 `--mcp-config=path` 与 `--mcp-config path` 两种拆分形态。

## [0.11.0] - 2026-05-29

### Added

- 命令行 `--` 透传: `jj [scene] 'prompt' -- <args>` 把 `--` 之后的 token 原样追加给底层 claude/codex (置于 scene 注入后、prompt 前), REPL 亦支持。
  - `src/parse.ts` 抽出 `--` 后的 tail args, `src/run.ts` 在 scene args 与 prompt 之间插入。

## [0.10.0] - 2026-05-29

### Fixed

- `--mcp-config` 指向的文件不存在 (如项目无 `.mcp.json`) 时不再让 claude 启动失败 — 透传前过滤掉缺失的文件路径, inline JSON 与存在的文件保留, `--strict-mcp-config` 保留。
  - `src/run.ts` 逐个 stat 检查 mcp-config 路径, 缺失即 drop; inline JSON 通过 JSON.parse 判定。

## [0.9.0] - 2026-05-19

### Changed

- **Breaking**: 非交互单跑默认输出从 raw `print` 翻转为 `stream-JSON` 渲染。
  - `src/cli.ts` mode 推导规则: 有 prompt + 无 `-p` → stream (原为 print)。

### Added

- `-p` / `--print` / `print`: 显式切回 raw print 透传模式。
  - `src/parse.ts` 新增 `-p` flag, `src/run.ts` 分派 print 分支。

### Removed

- `-s` / `--stream` / `stream`: 默认即 stream, flag 冗余。

## [0.8.0] - 2026-05-19

### Added

- prompt 顺序分段: 嵌入 `<<>>` 把 prompt 拆成 N 段独立 single-shot 串行执行, 每段全新 child。与 `--loop N>1` / `auto` / `refine` 互斥, stderr 标 `==> step i/N`。
  - `src/parse.ts` 按 `<<>>` split, `src/run.ts` 顺序循环 spawn, 每段独立 child.
- 任一段非 0 退出或异常仅 `[warn]` 并继续下一段, 返回最后一段 exit code。

## [0.7.2] - 2026-05-19

### Fixed

- `--loop N` 第 1 轮 child 非 0 退出会 break 丢失后续轮次; 现在 warn-continue, 必跑满 N 次。
- `--loop auto` / `refine` 子进程 exit≠0 立即中断; 现在同样 warn-continue, 由 `--max-iter` / `status=end` 决定终止。
  - `src/run.ts` `runFixedLoop` / `runAgentLoop` catch spawn 异常 + 非 0 exit 转 warn.

### Changed

- 移除 `--loop auto/refine` "连续 3 次 handoff 解析失败 abort" 规则, 改由 `--max-iter` 兜底。
- handoff 解析失败时 stderr 输出 `agent_output_tail`; loop 警告统一格式。
- `Bun.spawn` 异常保留原始 errno (ENOENT / EACCES), 不再一律改写为 "Command not found"。
- 子进程 stdout 流读取加 try/catch, 断流不冒泡到外层 loop。
- `runAgentLoop` 全程零成功 handoff 返回退出码 4。

## [0.7.1] - 2026-05-17

### Changed

- `--loop refine` handoff schema 收敛到 `{"status": "end" | "continue"}` 一字段, 移除 `iteration` / `summary`; `parseHandoff` 向后兼容。
  - `src/handoff.ts` refine schema minimal, `parseHandoff` 只取 status.

## [0.7.0] - 2026-05-17

### Added

- `--loop refine` 打磨式自动循环: 每轮全新 child, 只把原始 prompt 喂下一轮, 跨轮唯一信号是 agent 自决的 end/continue。复用 `--max-iter` 与 `/handoff`。
  - `src/run.ts` `runAgentLoop` 增 refine 分支, prompt 每轮重置为 original.
- `/handoff` 端点响应新增 `mode` 字段 (`auto` / `refine`)。

### Changed

- `--loop auto` 协议措辞微调 (自称"接力式"), 行为不变。
- 两模式协议片段完全隔离, 互不引用。

## [0.6.0] - 2026-05-17

### Added

- `--loop auto` 自动循环: 每轮全新 child, agent 末尾输出 `<<JJ_HANDOFF>>...<<JJ_HANDOFF_END>>` JSON baton, 父进程注入下一轮直到 status=end 或达 `--max-iter` (默认 100)。
  - `src/handoff.ts` sentinel 扫描 + JSON parse, `src/run.ts` `runAgentLoop` 循环 spawn.
- 本地观察端点: stderr 打印 `http://127.0.0.1:<port>/handoff`, 暴露 iteration/handoff/history 快照, 退出自动关闭。
  - `Bun.serve({ port: 0 })` OS 分配端口, 进程结束自动关闭.
- 退出码: end→0; max-iter→0+警告; 子进程非 0→透传; 连续 3 轮解析失败→3。

### Changed

- print 模式 stdout 从 `inherit` 改为 `pipe` + 透传, 以便扫 sentinel。

## [0.5.0] - 2026-05-17

### Added

- `--loop N`: 同一 single-shot 串行重跑 N 次, 任一非 0 立即中止。仅带 prompt 的非交互场景可用。
  - `src/run.ts` `runFixedLoop` 顺序 await spawn.

## [0.4.0] - 2026-05-17

### Changed

- **Breaking**: prompt 改为位置参数 `jjlauncher [scene] 'prompt'`, 依赖 shell 引号。
  - `src/parse.ts` 位置参数解析重写, mode 由 argv 形态推导.
- mode 由参数推导: 无 prompt→REPL; 有 prompt→print; 有 prompt + `-s`→stream。
- `install.sh` asset 下载改用 `curl --progress-bar`。

### Removed

- `-p` / `--print` / `print`: 由是否有 prompt 参数自动判定。
- `-e` / `--editor`: shell 引号即可。
- 多行交互输入 (`:q` 提交) 与 `readUserTextFromTerminal`。

## [0.3.0] - 2026-05-17

### Added

- 子命令 `update` / `upgrade`: 从 GitHub Release 拉最新二进制原子替换 (进度条、SHA256 校验、版本对照)。
  - `src/download.ts` fetch + `Bun.write` + `chmod +x` + `rename` 原子替换.
- 子命令 `uninstall`: 删除当前二进制。
- `install.sh`: 一键安装, 支持 `VERSION` / `INSTALL_DIR` / `REPO` 覆写。
- `.github/workflows/release.yml`: tag 触发自动构建 + 发布。
- `build.ts` 通过 `--define` 注入 `BUILD_NAME` / `BUILD_VERSION` / `BUILD_REPO`。
- `AGENTS.md`: 工程级 AI 协作文档。

### Changed

- **Breaking**: binary 名 `jj` → `jjlauncher`, 旧二进制需手动删除, 配置目录不变。
- 工程结构对齐 cli-template: `build.ts` 移至顶层、`tsconfig.json` 启用严格选项。
- 安装方式: `bun install -g` → GitHub Release + `install.sh`。
- `deploy.md`: tag 改 annotated + `tag.gpgsign`。
- AI-only 声明移至 `AGENTS.md`, `README.md` 仅用户向。

### Removed

- `scripts/install-global.ts`、`features/bun-migration.md`、`example/config.json`、`LICENSE`。
- 未使用依赖 (`@anthropic-ai/sdk` 等), 改用 `@types/bun`。

## [0.1.0] - 2026-03-15

### Added

- 三种运行模式: `jj [scene]` 交互、`jj -p [scene]` 文本输入、`jj -s [scene]` 流式 JSON。
- Codex 前缀 `.` (例: `jj .d`)。
- 内置 scene: `default` / `ai-expert` / `it-expert` / `code-expert` / `address`。
- 首次运行初始化 `~/.config/cli-prompt-launcher/`。
- Claude / Codex 流事件格式化输出。

[0.12.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.11.1...v0.12.0
[0.11.1]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.7.2...v0.8.0
[1.0.1]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.12.0...v1.0.0
[0.7.2]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/yigegongjiang/jj-prompt-launcher/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/yigegongjiang/jj-prompt-launcher/releases/tag/v0.1.0
