```When Editing
本文档作用: 面向开发者的发版记录; CHANGELOG.md 的超集, 1:1 镜像 + 技术变更子项
遵循 AGENTS.md 文档编写规范
- 每条主项 = CHANGELOG.md 对应条目 (原文), 下方缩进子项承载技术变更
- 子项 MAY 写路径 / 函数 / 机制; ≤ 1 行
```

# Changelog (developer, follow [CHANGELOG.md](./CHANGELOG.md))

## [1.5.1] - 2026-10-05

### Changed

- `ext on|off` 的 Claude 项目开关统一写入优先级最高的 `.claude/settings.local.json`。
  - 依据 Claude settings 优先级 managed > `--settings` > project local > shared project > user; `enabledPlugins` 按子键合并 (实测 `claude plugin list`)。

### Fixed

- `ext` 改配置更稳: 内联 / 点分写法原位修改不再被跳过或丢失, 值类型异常时报错而不覆盖, 软链配置写到真实文件, 备份与新文件保留原权限。
  - `ext_codex.rs`: `skills_rules` + `rules_mut` 同时支持 `[[skills.config]]` 与内联数组 (旧实现把内联 `skills = { config = [...] }` 转 AoT 后丢数据); `preserves` 护栏比对改写前后 TOML, 除 `enabled` / 列表尾部新增外不得变化。
  - `ext.rs`: 写前统一比对全部文件再写; `create_new` 临时文件 + `sync_all` + 沿用权限 + rename; `real_path` 跟随软链; 备份用 `fs::copy` 保权限。参照 cc-switch `config.rs` 原子写。
  - `set_enabled` / `obj_mut` 遇非表 / 非 object 值返回错误。
- Codex 全局 skill 开关改用名称规则, skill 所在路径随版本变化后不再自动恢复为开启。
  - 已有 path / name 规则仍原位改; 只有新增规则用 `name`。

### Added

- `docs/agent-ext-config.md`: Claude / Codex 的 MCP / skill / plugin 配置层级、优先级、坑与本项目写配置的稳定性约束。
  - 同时提交本项目自身的 `.claude/settings.local.json` / `.codex/config.toml` / `.mcp.json` 作测试样例 (`.gitignore` 放行 `settings.local.json`)。

## [1.5.0] - 2026-10-05

### Added

- 新增 `ext` 子命令: 一条命令全局开 / 关 Claude Code + Codex 的全部 MCP / skill / plugin (`ext global on|off`, 支持 `--dry-run`)。
  - `ext.rs` 分发 + plan/apply 分离 (`FileEdit` = before/after/changes), `main.rs` 在 scene 解析前路由 `ext`。
  - Codex 清单走 `codex app-server` `skills/list` + `plugin/list` (cwd=$HOME, 无 `-c`, 60s 超时), 过滤 repo / plugin skill 与 remote plugin; `toml_edit` 保格式改写。
  - Claude MCP: `claude.args` 绝对 `--mcp-config` 文件 = 全局开集, 关闭时定义存 `mcp-catalog.json`; `@builtin` plugin / Codex `features.*` 不动。
- 在任意项目目录用 `ext on|off` 定点开关 (fzf 多选或直接给名字), 配置写进该项目; `ext` / `ext ls` 一览全局 + 项目 + 生效状态。
  - Claude 项目键写入已持有该键的 `settings.local.json` (Claude 以 local 为准), 否则 `settings.json`。
  - Codex 项目 mcp 仅允许全局已定义的名字 (enabled-only 表靠与全局定义合并, 实测 codex-cli 0.160.0); 非 trusted 项目给 `[note]`。
- 全局写入自动留首次原始备份与上一次备份。
  - `<file>.jj-orig` 只写一次 + `<file>.jj-bak` 每次覆盖; 临时文件 + rename; 写前比对 before, 被改即中止。

## [1.4.2] - 2026-10-05

### Added

- 新配置 `claude.user_skills_off`: 开启后 Claude 启动时隐藏全部用户级 skill, 项目在自己的 `.claude/settings.json` 用 `skillOverrides` 按需打开; 新装的用户 skill 自动纳入; `--pre` 切换目录后同样生效。
  - `skills.rs`: 枚举 `$CLAUDE_CONFIG_DIR|~/.claude/skills/*/SKILL.md` (frontmatter `name` 优先), 减去 cwd `.claude/settings{,.local}.json` `skillOverrides` 键 + cwd `.claude/skills` 同名, 生成 `--settings` JSON, 经 `build_final_args` 的 project_args 槽注入。
  - 实测 claude 2.1.289: `--settings` 优先级高于 project/local (故必须做减法), 项目 settings 只读 cwd, 插件 skill 不受 `skillOverrides` 影响。
  - `--pre`: 占位符 `<claude-user-skills-off>` 由 `shell::emit_claude_skills` 回调隐藏子命令 `__claude-user-skills-settings` 现算; config / 透传已含 `--settings` 时 `[warn]` 跳过。

## [1.4.1] - 2026-10-05

### Added

- Codex 启动时读取项目 `.codex/config.toml` 的 `[[skills.config]]` 并追加到启动参数: 项目级 skill 开关生效 (Codex 自身只认全局配置); `--pre` 切换目录后同样生效。
  - 依据: codex-rs `skill_config_rules_from_stack` 只收 `User` / `SessionFlags` 层 (codex-cli 0.160.0 实测); `-c` 落 SessionFlags 层, 叠加在全局规则之上。
  - `codex_project.rs`: 项目根 = 最近 `.git` 祖先 (同 Codex 默认 `project_root_markers`), 根 -> cwd 收集, 相对 `path` 按 `.codex/` 转绝对; 新增 `toml` 依赖做解析 + 序列化。
  - `--pre`: argv 放占位 `<codex-project-skills>`, `shell.rs` 生成脚本在 pre 后回调隐藏子命令 `__codex-project-skills` 取值。
  - 只转发 skill 规则: 其余键若经 `-c` 注入会绕过 Codex 信任门与项目配置 denylist。

## [1.4.0] - 2026-08-17

### Added

- 新增第三个引擎 agy (Google Antigravity CLI): scene token 加前缀 `,` 即走 agy (`,d` / `,it`), 与 Claude Code (无前缀) / Codex (`.`) 完全同一套用法。
  - `scenes.rs` `Engine` 加 `Agy` 变体, `split_engine_prefix` 加 `,` 分支 (`.` / `,` 均对 sh/bash/zsh 无语法意义, 免引号); `config.rs` 加 `agy` 段 (`args` = `--dangerously-skip-permissions`, `print` = `--print-timeout 24h` 防 agy 默认 5m 掐断长任务, `stream` = `--output-format stream-json`)。
  - `format_agy.rs` 新增 stream-json 渲染器: `init` 头 / `agent_response.text_delta` 逐片输出 / `tool` 首见打头 + DONE 打 `[tool_result]` / `result` 只出元信息 (其 `response` 与正文重复)。
- `--loop relay` / `--loop refine` / `<<>>` 分段 / `--pre` / `--` 透传在 agy 上一并可用; `scenes.default` 支持 `,it` 把 agy 设为默认引擎。
  - agy 走 `run.rs` 同一条 `LaunchPlan` 路径, loop 的 handoff 协议以 system_suffix 拼进 scene 文本 (即 `<system_instructions>` 内), sentinel 结构全 ASCII 不受 delta 切片影响。
- agy 没有 system prompt 参数, scene 以 `<system_instructions>` 包裹进 prompt 注入; REPL 下作为首轮消息注入 (agent 只回一行「就位」再等指令)。
  - 已核对 `agy --help` + antigravity.google/docs/cli/headless (2026-08-17): 无 `--system-prompt` / `--append-system-prompt` 类参数, `--agent` 需预置 agent 定义文件, 故不采用。
  - `build_agy_prompt()` 生成 prompt; `-p` / `-i` 由代码结构性追加 (Go flag 语义下 prompt 是该 flag 的值, 必须紧邻), 不放进 config.json。

### Changed

- 流式渲染 (默认模式) 下 agy 自身按字节切分输出增量, CJK 字符跨切片会被它替换成 `U+FFFD`; 需要完整文本时用 `-p`。
  - 上游 encoder 行为 (实测 delta 里含字面 `EF BF BD`, `result.response` 干净), 本项目侧不可修复; 未做「末尾重打干净全文」的补偿以免正文重复。

### Fixed

- config.json 中整段缺失的引擎不再按「零参数」启动 (会误开 REPL 而非单次执行), 改用内置默认参数 — 老版本写的 config 不改也能直接用 agy。
  - `get_configured_args` 缺段时回退 `seed_config()` (`DEFAULT_CONFIG_JSON` 解析, `OnceLock` 缓存, 单一信源); 显式空对象 `{}` 仍解析为 `Some(EngineConfig)` → 保持「无参数」语义。

## [1.3.1] - 2026-08-04

### Changed

- `ai-expert` scene prompt 重写: 准确性约束前置 (数字 / 来源 / API 字符串未经检索或确知则不给, 推断须标注可信级别, 能力与定价锚定版本日期)。
  - `scenes/ai-expert.md` 与上游 `jj-prompts/user-prompts/ai-expert.md` 逐字对齐 (单一信源, 其余 4 个 scene 已一致)。
- `ai-expert` 输出不再套固定模板 (原五步流程 + RPP 框架), 由回答自选形式, 覆盖面补齐后训练 / 推理经济性 / MCP / evals。
  - 段落结构 `Expertise` / `Rules` / `Workflow` → `Accuracy first` / `Form`; 删除强制 `[Author, Year]` 引用格式要求。
- 已落盘的 `~/.config/jj-agentic-launcher/scenes/ai-expert.md` 不会被自动覆盖, 需手动同步新版内容。
  - `init.rs` 仅在 config.json 缺失时落盘, 设计如此, 不加覆盖逻辑。

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

[1.5.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.5.0...v1.5.1
[1.5.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.4.2...v1.5.0
[1.4.2]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.3.1...v1.4.0
[1.3.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.3.0...v1.3.1
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
