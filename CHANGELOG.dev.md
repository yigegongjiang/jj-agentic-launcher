```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

## [1.3.0] - 2026-07-29

### Added

- 默认 scene 可动态配置: `config.json` → `scenes.default` 支持别名 (`it`) / scene 文件名 (`it-expert`) / `.` 前缀改默认引擎为 Codex (`.it`), 改完即生效。
  - `scenes.rs` 新增 `default_scene()` (`OnceLock` 缓存, 进程内解析一次) + `split_engine_prefix()` / `resolve_scene_id()` 拆分; `resolve_scene_token()` 收 `None` / `""` / 裸 `.` 三种省略形态。
  - 默认 token 解析不复用 `resolve_scene_token`, 单向调用避免「空 → 默认 → 空」递归; `config.rs` `get_default_scene_id` → `get_default_scene_token`。
- scene 参数全面可省略: 裸命令进默认 scene REPL; `-p 'prompt'` / `--loop N 'prompt'` 单参数即 prompt (旧版报错); 空 token `''` = 省略; 裸 `.` = 默认 scene 强制 Codex。
  - `parse.rs` 0/1/2 args 三分支重写为一次 `(ResolvedScene, Option<prompt>)` 解析; `needs_prompt = want_print || loop != Fixed(1)` 作为「单参数必为 prompt」的判据。
- 单参数不匹配任何 scene 时当 prompt 跑, stderr `[info]` 提示实际用的 scene (scene 名打错立刻可见); 两参数时第一个仍必须是 scene。
  - `truncate_for_log()` 把参数压成单行 ≤40 字符 (按 `chars` 截断, 不切 UTF-8 边界)。
- `help` 展示当前生效的默认 scene + 引擎。

### Fixed

- `scenes.default` 此前只影响裸命令的交互模式, 且只认 scene 文件名 (写别名会启动失败); 现全入口生效, 值非法则 `[warn]` 回退内置 `default` 而非阻断。
  - 旧 0-args 分支硬编码 `Engine::Claude` + 直接把 config 值当 `scene_id`; 旧 `resolve_scene_token` 里裸 `.` 硬编码回退别名 `"d"` (删掉该别名即失效) 一并移除。

## [1.2.1] - 2026-07-28

### Changed

- `scenes/*.md` 全量重写 (compile-time `include_str!` 嵌入, 属交付物变更 → 跟版本发布)。
  - 结构统一: 角色/约束改英文指令, `address` 增 `<examples>` few-shot, 语言输出要求下沉到 prompt 顶部。
  - `init.rs` 仅在 config.json 缺失时落盘, 老用户 config 目录不受影响 (设计如此, 不加覆盖逻辑)。

## [1.2.0] - 2026-07-28

### Changed

- **Breaking**: 全项目改名 `jj-prompt-launcher` → `jj-agentic-launcher` (命令名 + binary 产物名 + repo 名统一)。旧命令需手动删除: `rm ~/.local/bin/jj-prompt-launcher`。
  - `Cargo.toml` `package.name` / `[[bin]].name` / `repository`, `src/meta.rs` `REPO` slug, `src/parse.rs` usage 串, `scripts/install*.sh` `BIN_NAME` / `REPO`, `.github/workflows/release.yml` 产物名全部同步。
  - GitHub repo `yigegongjiang/jj-prompt-launcher` → `jj-agentic-launcher` (旧 URL 由 GitHub 永久重定向); 本地 remote + 工作目录同步改名。
  - release asset 名契约随之变为 `jj-agentic-launcher-darwin-{arm64,x64}`; 旧版本二进制的 `update` 子命令仍指向旧 slug, 经 GitHub 重定向可继续工作, 但会拉到旧 asset 名 — 旧版需重新走 `install.sh`。
- **Breaking**: config 目录 `~/.config/jj-prompt-launcher/` → `~/.config/jj-agentic-launcher/`, 旧目录需手动 `mv` 迁移。
  - `src/config.rs` `get_config_dir()` 单点改名; 不加自动迁移分支 (与 0.12.0 改名一致, 避免留长期兼容代码)。

### Fixed

- 安装命令 URL 修正为仓库内实际路径 (`main/scripts/install.sh`), 旧 URL 404。
  - `install.sh` 自 0.12.0 起就在 `scripts/` 下, README 的 raw URL 未同步。

## [1.1.0] - 2026-07-25

### Added

- `--pre '<cmd>'`: 先在交互 shell 里执行 `<cmd>`, 引擎随后顶替该 shell 继续跑, 继承它留下的 cwd / 环境变量 / source 状态。`--pre 'j api'`、`--pre 'cd $(fd -t d | fzf)'`、`--pre 'source .venv/bin/activate && cd backend'` 均生效, fzf 这类 TUI 选择器可正常交互。
  - 新增 `src/shell.rs`: `quote` (POSIX 单引号, `'` → `'\''`) + `build_script` 生成 `$SHELL -i -c` 脚本; `run.rs` 的 `build_command` 在 `pre_cmd` 为 `None` 时走原 `Command::new(binary)` 路径, 零回归面。
  - 脚本形态: `export TERM=<真值>` → `{ <cmd> } 1>&2 || exit $?` → `set --` 逐段构建 argv → `exec <engine> "$@"`; `exec` 保证进程树扁平, 信号 / exit code 直传。
  - spawn 时置 `TERM=dumb`: rc 里的 shell integration (iTerm2 等) 启动即往 stdout 吐 OSC 序列, 会污染 stream-JSON 管道; 脚本首行恢复真 `TERM` 供 fzf 用 terminfo。
  - 非交互模式 stdin 由 `Stdio::null()` 改为 `inherit()`, 引擎的 stdin 改由脚本末尾 `</dev/null` 掐断 — pre 阶段保留 tty/管道供选择器读候选。
  - `sanitize_mcp_config` 加 `defer_relative`: `--pre` 下父进程 cwd ≠ 引擎 cwd, 相对路径原样保留, 改由脚本内 `[ -f ]` 循环守卫 (一组只产生一个 `--mcp-config`, 避免重复 flag 相互覆盖)。
- `--pre` 在每次引擎启动前执行 (含 `--loop` 每轮与 `<<>>` 每段); `<cmd>` 非零退出直接终止, 不启动引擎。仅命令行传入, config.json 不支持配置。
  - `pre_cmd` 挂在 `Invocation` 上而非 `run()` 前置一次: shell 状态无法跨进程存活, 只跑首轮会让第 2..N 轮回退到原 cwd。
  - `parse_flags` 返回值由 4-tuple 改为 `Flags` struct; `--pre` 空值 / 缺值走 `AppError::Usage`。

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

[1.3.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.2.1...v1.3.0
[1.2.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.1.0...v1.2.0
[0.12.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.11.1...v0.12.0
[0.11.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.7.2...v0.8.0
[1.1.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.12.0...v1.0.0
[0.7.2]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/yigegongjiang/jj-agentic-launcher/releases/tag/v0.1.0
