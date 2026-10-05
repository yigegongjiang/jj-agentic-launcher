# Claude Code / Codex 的 MCP · skill · plugin 配置机制

本项目 `ext` 子命令 + Codex skill 转发 + `claude.user_skills_off` 的事实依据. 核对版本: claude 2.1.289 / codex-cli 0.160.0 (2026-10-05).

## 速查

<!-- prettier-ignore -->
| 引擎 | 类别 | 全局位置 | 项目位置 | 项目层原生生效 |
| --- | --- | --- | --- | --- |
| Claude | plugin | `~/.claude/settings.json` `enabledPlugins` | `.claude/settings.local.json` 同键 | 是 |
| Claude | skill | `skillOverrides` (无全关开关) | `.claude/settings.local.json` `skillOverrides` | 是 |
| Claude | mcp | `~/.claude.json` `mcpServers` | `.mcp.json` | 需批准 / `--mcp-config` |
| Codex | plugin | `~/.codex/config.toml` `plugins."<id>".enabled` | `.codex/config.toml` 同键 | 是 (需 trusted) |
| Codex | skill | `[[skills.config]]` path / name 规则 | 同键 | 否 (被忽略) |
| Codex | mcp | `[mcp_servers.<name>]` | 同键 | 是 (需 trusted) |

## Claude Code

- settings 优先级 (高 -> 低): managed > `--settings` > `.claude/settings.local.json` > `.claude/settings.json` > `~/.claude/settings.json`. 项目写入 MUST 用 `settings.local.json`, 才压得住共享 `settings.json`.
- 合并: 同名标量取高层; 列表 (`permissions.allow` 等) 跨层拼接; `enabledPlugins` / `skillOverrides` 按子键合并 (实测 `claude plugin list`: 项目 local 只写 `codex@openai-codex: true`, 它变 enabled, 其余 plugin 仍按用户层 disabled).
- 项目 settings 只读 cwd 的 `.claude/`, 不上溯父目录 / git root.
- skill: `skillOverrides: {"<name>": "off"|"on"}`; plugin 自带的 skill 不受它控制 (跟 plugin 走). 无「用户 skill 全关」开关 -> 启动器每次启动枚举 `~/.claude/skills/*` 生成 `--settings` off 列表, 减去项目已列出的名字 (`src/skills.rs`).
- plugin: `enabledPlugins: {"<id>": bool}`; 新装默认开. `@builtin` (telemetry 等) 属 Claude 内部, 不纳入管理.
- mcp: `--strict-mcp-config` = 只认 `--mcp-config` 给的来源, 忽略 `~/.claude.json` user/local scope 与 `.mcp.json` 自动发现. 启动器传 `--mcp-config <全局文件> .mcp.json` + strict, 所以全局文件 = 全局开集, `.mcp.json` = 项目开集; 关闭的定义存 `~/.config/jj-agentic-launcher/mcp-catalog.json`.

## Codex

- 配置层 (低 -> 高): user `~/.codex/config.toml` -> project `.codex/config.toml` (项目根 -> cwd 逐层, 根 = 最近含 `.git` 的祖先, 无则仅 cwd) -> `-c` (SessionFlags, 最高). TOML 表跨层深合并.
- project 层 MUST trusted (`[projects."<path>"] trust_level = "trusted"`, 精确匹配 cwd / 项目根 / repo 根, 不看祖先); 未 trusted 整层不加载.
- project 层 denylist (源码 `PROJECT_LOCAL_CONFIG_DENYLIST`): `openai_base_url` / `chatgpt_base_url` / `model_provider(s)` / `notify` / `profile(s)` / `otel` 等; `-c` 无此限制 -> 不可把项目配置整份转成 `-c`.
- skill: `skill_config_rules_from_stack` 只读 User + SessionFlags 层, project 层 `[[skills.config]]` 被静默丢弃 -> 启动器把它转成 `-c skills.config=[...]` (`src/codex_project.rs`). 规则按层顺序叠加, 后者覆盖同一 skill; name 规则命中所有同名 skill (如用户版 + 系统版 `skill-creator`). path 规则会因 skill 搬家 (软链指向带版本号的 app 路径) 失效 -> 新增规则用 name. `[skills.bundled] enabled = false` 一次关全部系统 skill (project 层可用).
- plugin: 本地 plugin (`openai-bundled` / `openai-primary-runtime`) 按 `plugins."<id>".enabled`; remote plugin (`@openai-curated-remote`) 的启停在账号服务端, 加载时服务端状态覆盖本地配置, 只能 `features.remote_plugin = false` 整类关. plugin 自带 MCP 若 manifest 写 `enabled: false`, 用户配置只能收紧不能放宽 (`server.enabled &= policy.enabled`).
- mcp: 无 `--strict-mcp-config` 对等物; 逐个 `enabled = false`. project 层只写 `[mcp_servers.x] enabled = true` 可行 (与全局定义深合并), 但全局未定义该名时缺 transport -> 配置错误.
- 全关类开关 (`--disable plugins` / `features.apps`) 都在 `-c` 最高层, 项目无法再单独打开 -> 「全关 + 项目白名单」只能靠逐项 `enabled`.
- 清单: `codex app-server` JSON-RPC (`initialize` -> `skills/list` / `plugin/list`) 比自己扫目录可靠; cwd 设 `$HOME` 且不带 `-c`, 拿到的才是用户层状态.

## 写配置的稳定性约束 (`src/ext.rs`)

参照 [cc-switch](https://github.com/farion1231/cc-switch) (140k★, 管理 Claude Code / Codex 配置) 的 `config.rs` 原子写与 `live/patch/toml.rs` 补丁器:

- TOML 用 `toml_edit` 改值, 注释 / 空行 / 键序 / 内联写法原样保留; `mcp_servers = {...}` / `skills = { config = [...] }` / 点分键均原位改, 不转换形态 (内联表里放不下 `[[...]]`, 转换会丢数据).
- 护栏: 每次 TOML 改写后解析前后两版, 除 `enabled` 与列表尾部新增外数据 MUST 完全一致, 否则不写.
- 值类型不符 (如 `plugins = "x"`, JSON 段非 object) 报错, 不覆盖用户数据.
- 原子写: 同目录 `create_new` 独占临时文件 -> 写入 -> `fsync` -> 沿用目标权限位 -> `rename`; 失败删临时文件.
- 软链配置 (dotfiles 管理) 写到链接目标, 不把软链替换成普通文件.
- 并发: 计划时记下原文; 写前逐个比对, 任一文件被改 (引擎自己在写) 则整体中止, 一个不写.
- 备份: `<file>.jj-orig` 首次写入后永不覆盖 + `<file>.jj-bak` 每次覆盖, `fs::copy` 保留权限位; 文件数恒定, 不随时间增长.

## 来源

- Claude settings 优先级 / 列表合并: https://code.claude.com/docs/en/settings (2026-10-05)
- Claude `--strict-mcp-config` / `--settings`: https://code.claude.com/docs/en/cli-reference (2026-10-05)
- Codex 源码: `codex-rs/config/src/skills_config.rs`, `codex-rs/config/src/loader/mod.rs`, `codex-rs/core-plugins/src/loader.rs`, `codex-rs/core-plugins/src/remote.rs` (openai/codex main, 2026-10-05)
- Codex remote plugin 忽略本地 `enabled = false`: https://github.com/openai/codex/issues/28443
