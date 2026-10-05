```When Editing
本文档作用: 面向使用者的发版记录; 只写用户感受得到的变化, MUST NOT 写技术细节 (→ CHANGELOG.dev.md)
遵循 AGENTS.md 文档编写规范
- 写: 新功能 / 行为修复 / 体验 / 安全 / 命令迁移
- MUST NOT 写: 文件路径 / 函数名 / 组件名 / 依赖包名 / 重构细节
- 单条 ≤ 2 行, 单版本 ≤ 5 条; 段落: Added / Changed / Fixed / Removed / Security
- 无用户可感知变化 → 占位: `跟随版本同步发布`
```

# Changelog

[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) + [SemVer](https://semver.org/).

## [1.5.2] - 2026-10-05

### Changed

- `ext on|off` 的 fzf 多选更醒目: 已选项显示绿色 ✓, `Ctrl-A` 一键反选当前筛选出的全部条目。

## [1.5.1] - 2026-10-05

### Changed

- `ext on|off` 的 Claude 项目开关统一写入优先级最高的 `.claude/settings.local.json`。

### Fixed

- `ext` 改配置更稳: 内联 / 点分写法原位修改不再被跳过或丢失, 值类型异常时报错而不覆盖, 软链配置写到真实文件, 备份与新文件保留原权限。
- Codex 全局 skill 开关改用名称规则, skill 所在路径随版本变化后不再自动恢复为开启。

## [1.5.0] - 2026-10-05

### Added

- 新增 `ext` 子命令: 一条命令全局开 / 关 Claude Code + Codex 的全部 MCP / skill / plugin (`ext global on|off`, 支持 `--dry-run`)。
- 在任意项目目录用 `ext on|off` 定点开关 (fzf 多选或直接给名字), 配置写进该项目; `ext` / `ext ls` 一览全局 + 项目 + 生效状态。
- 全局写入自动留首次原始备份与上一次备份。

## [1.4.2] - 2026-10-05

### Added

- 新配置 `claude.user_skills_off`: 开启后 Claude 启动时隐藏全部用户级 skill, 项目在自己的 `.claude/settings.json` 用 `skillOverrides` 按需打开; 新装的用户 skill 自动纳入; `--pre` 切换目录后同样生效。

## [1.4.1] - 2026-10-05

### Added

- Codex 启动时读取项目 `.codex/config.toml` 的 `[[skills.config]]` 并追加到启动参数: 项目级 skill 开关生效 (Codex 自身只认全局配置); `--pre` 切换目录后同样生效。

## [1.4.0] - 2026-08-17

### Added

- 新增第三个引擎 agy (Google Antigravity CLI): scene token 加前缀 `,` 即走 agy (`,d` / `,it`), 与 Claude Code (无前缀) / Codex (`.`) 完全同一套用法。
- `--loop relay` / `--loop refine` / `<<>>` 分段 / `--pre` / `--` 透传在 agy 上一并可用; `scenes.default` 支持 `,it` 把 agy 设为默认引擎。
- agy 没有 system prompt 参数, scene 以 `<system_instructions>` 包裹进 prompt 注入; REPL 下作为首轮消息注入 (agent 只回一行「就位」再等指令)。

### Changed

- 流式渲染 (默认模式) 下 agy 自身按字节切分输出增量, CJK 字符跨切片会被它替换成 `U+FFFD`; 需要完整文本时用 `-p`。

### Fixed

- config.json 中整段缺失的引擎不再按「零参数」启动 (会误开 REPL 而非单次执行), 改用内置默认参数 — 老版本写的 config 不改也能直接用 agy。

## [1.3.1] - 2026-08-04

### Changed

- `ai-expert` scene prompt 重写: 准确性约束前置 (数字 / 来源 / API 字符串未经检索或确知则不给, 推断须标注可信级别, 能力与定价锚定版本日期)。
- `ai-expert` 输出不再套固定模板 (原五步流程 + RPP 框架), 由回答自选形式, 覆盖面补齐后训练 / 推理经济性 / MCP / evals。
- 已落盘的 `~/.config/jj-agentic-launcher/scenes/ai-expert.md` 不会被自动覆盖, 需手动同步新版内容。

## [1.3.0] - 2026-07-29

### Added

- 默认 scene 可动态配置: `config.json` → `scenes.default` 支持别名 (`it`) / scene 文件名 (`it-expert`) / `.` 前缀改默认引擎为 Codex (`.it`), 改完即生效。
- scene 参数全面可省略: 裸命令进默认 scene REPL; `-p 'prompt'` / `--loop N 'prompt'` 单参数即 prompt (旧版报错); 空 token `''` = 省略; 裸 `.` = 默认 scene 强制 Codex。
- 单参数不匹配任何 scene 时当 prompt 跑, stderr `[info]` 提示实际用的 scene (scene 名打错立刻可见); 两参数时第一个仍必须是 scene。
- `help` 展示当前生效的默认 scene + 引擎。

### Fixed

- `scenes.default` 此前只影响裸命令的交互模式, 且只认 scene 文件名 (写别名会启动失败); 现全入口生效, 值非法则 `[warn]` 回退内置 `default` 而非阻断。

## [1.2.1] - 2026-07-28

### Changed

- 内置 scene prompt 全面重写 (`address` / `ai-expert` / `code-expert` / `it-expert` / `default`): 英文指令 + 示例驱动, 输出更稳定。
- 已落盘的 `~/.config/jj-agentic-launcher/scenes/*.md` 不会被自动覆盖, 需手动同步新版内容。

## [1.2.0] - 2026-07-28

### Changed

- **Breaking**: 全项目改名 `jj-prompt-launcher` → `jj-agentic-launcher` (命令名 + binary 产物名 + repo 名统一)。旧命令需手动删除: `rm ~/.local/bin/jj-prompt-launcher`。
- **Breaking**: config 目录 `~/.config/jj-prompt-launcher/` → `~/.config/jj-agentic-launcher/`, 旧目录需手动 `mv` 迁移。

### Fixed

- 安装命令 URL 修正为仓库内实际路径 (`main/scripts/install.sh`), 旧 URL 404。

## [1.1.0] - 2026-07-25

### Added

- `--pre '<cmd>'`: 先在交互 shell 里执行 `<cmd>`, 引擎随后顶替该 shell 继续跑, 继承它留下的 cwd / 环境变量 / source 状态。`--pre 'j api'`、`--pre 'cd $(fd -t d | fzf)'`、`--pre 'source .venv/bin/activate && cd backend'` 均生效, fzf 这类 TUI 选择器可正常交互。
- `--pre` 在每次引擎启动前执行 (含 `--loop` 每轮与 `<<>>` 每段); `<cmd>` 非零退出直接终止, 不启动引擎。仅命令行传入, config.json 不支持配置。

## [1.0.1] - 2026-07-25

### Changed

- `--loop auto` 改名 `--loop relay`, 不保留别名 (旧写法直接报错)。`auto` 描述的是"次数自动", 与 `refine` 不在同一语义维度, 且掩盖了 refine 同样由 agent 自决停止; `relay` 直指"接力传 baton", 与 `refine` (零上下文重做) 成对。
- `/handoff` 端点 `mode` 字段值同步 `auto` → `relay`。

## [1.0.0] - 2026-07-23

### Changed

- 运行时从 Bun/TypeScript 迁移到 Rust: 单文件二进制体积 ~50MB → ~0.5MB, 启动更快。命令行 / scene / config / loop / handoff 行为完全不变, `~/.config/jj-prompt-launcher/` 无需迁移。
- `update` 子命令下载改用系统 `curl` (与 install.sh 一致), 进度条为 curl 原生样式; macOS 自带 `curl`, 无需额外安装。

## [0.12.0] - 2026-07-23

### Changed

- **Breaking**: 全项目改名 `jjlauncher` / `cli-prompt-launcher` → `jj-prompt-launcher` (命令名 + `package.json#name` + binary 产物名 + repo 名统一)。
- **Breaking**: config 目录 `~/.config/cli-prompt-launcher/` → `~/.config/jj-prompt-launcher/`, 旧目录需手动 `mv` 迁移。

## [0.11.1] - 2026-06-09

### Fixed

- `--mcp-config=<path>` 等号连接格式现在正确执行文件存在性检查, 与空格分隔格式行为一致。

## [0.11.0] - 2026-05-29

### Added

- 命令行 `--` 透传: `jj [scene] 'prompt' -- <args>` 把 `--` 之后的 token 原样追加给底层 claude/codex (置于 scene 注入后、prompt 前), REPL 亦支持。

## [0.10.0] - 2026-05-29

### Fixed

- `--mcp-config` 指向的文件不存在 (如项目无 `.mcp.json`) 时不再让 claude 启动失败 — 透传前过滤掉缺失的文件路径, inline JSON 与存在的文件保留, `--strict-mcp-config` 保留。

## [0.9.0] - 2026-05-19

### Changed

- **Breaking**: 非交互单跑默认输出从 raw `print` 翻转为 `stream-JSON` 渲染。

### Added

- `-p` / `--print` / `print`: 显式切回 raw print 透传模式。

### Removed

- `-s` / `--stream` / `stream`: 默认即 stream, flag 冗余。

## [0.8.0] - 2026-05-19

### Added

- prompt 顺序分段: 嵌入 `<<>>` 把 prompt 拆成 N 段独立 single-shot 串行执行, 每段全新 child。与 `--loop N>1` / `auto` / `refine` 互斥, stderr 标 `==> step i/N`。
- 任一段非 0 退出或异常仅 `[warn]` 并继续下一段, 返回最后一段 exit code。

## [0.7.2] - 2026-05-19

### Fixed

- `--loop N` 第 1 轮 child 非 0 退出会 break 丢失后续轮次; 现在 warn-continue, 必跑满 N 次。
- `--loop auto` / `refine` 子进程 exit≠0 立即中断; 现在同样 warn-continue, 由 `--max-iter` / `status=end` 决定终止。

### Changed

- 移除 `--loop auto/refine` "连续 3 次 handoff 解析失败 abort" 规则, 改由 `--max-iter` 兜底。
- handoff 解析失败时 stderr 输出 `agent_output_tail`; loop 警告统一格式。
- `Bun.spawn` 异常保留原始 errno (ENOENT / EACCES), 不再一律改写为 "Command not found"。
- 子进程 stdout 流读取加 try/catch, 断流不冒泡到外层 loop。
- `runAgentLoop` 全程零成功 handoff 返回退出码 4。

## [0.7.1] - 2026-05-17

### Changed

- `--loop refine` handoff schema 收敛到 `{"status": "end" | "continue"}` 一字段, 移除 `iteration` / `summary`; `parseHandoff` 向后兼容。

## [0.7.0] - 2026-05-17

### Added

- `--loop refine` 打磨式自动循环: 每轮全新 child, 只把原始 prompt 喂下一轮, 跨轮唯一信号是 agent 自决的 end/continue。复用 `--max-iter` 与 `/handoff`。
- `/handoff` 端点响应新增 `mode` 字段 (`auto` / `refine`)。

### Changed

- `--loop auto` 协议措辞微调 (自称"接力式"), 行为不变。
- 两模式协议片段完全隔离, 互不引用。

## [0.6.0] - 2026-05-17

### Added

- `--loop auto` 自动循环: 每轮全新 child, agent 末尾输出 `<<JJ_HANDOFF>>...<<JJ_HANDOFF_END>>` JSON baton, 父进程注入下一轮直到 status=end 或达 `--max-iter` (默认 100)。
- 本地观察端点: stderr 打印 `http://127.0.0.1:<port>/handoff`, 暴露 iteration/handoff/history 快照, 退出自动关闭。
- 退出码: end→0; max-iter→0+警告; 子进程非 0→透传; 连续 3 轮解析失败→3。

### Changed

- print 模式 stdout 从 `inherit` 改为 `pipe` + 透传, 以便扫 sentinel。

## [0.5.0] - 2026-05-17

### Added

- `--loop N`: 同一 single-shot 串行重跑 N 次, 任一非 0 立即中止。仅带 prompt 的非交互场景可用。

## [0.4.0] - 2026-05-17

### Changed

- **Breaking**: prompt 改为位置参数 `jjlauncher [scene] 'prompt'`, 依赖 shell 引号。
- mode 由参数推导: 无 prompt→REPL; 有 prompt→print; 有 prompt + `-s`→stream。
- `install.sh` asset 下载改用 `curl --progress-bar`。

### Removed

- `-p` / `--print` / `print`: 由是否有 prompt 参数自动判定。
- `-e` / `--editor`: shell 引号即可。
- 多行交互输入 (`:q` 提交) 与 `readUserTextFromTerminal`。

## [0.3.0] - 2026-05-17

### Added

- 子命令 `update` / `upgrade`: 从 GitHub Release 拉最新二进制原子替换 (进度条、SHA256 校验、版本对照)。
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

[1.5.2]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.5.1...v1.5.2
[1.5.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.5.0...v1.5.1
[1.5.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.4.2...v1.5.0
[1.4.2]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.3.1...v1.4.0
[1.3.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.3.0...v1.3.1
[1.3.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.2.1...v1.3.0
[1.2.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.12.0...v1.0.0
[0.12.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.11.1...v0.12.0
[0.11.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/yigegongjiang/jj-agentic-launcher/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/yigegongjiang/jj-agentic-launcher/releases/tag/v0.1.0
