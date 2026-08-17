```When Editing
本文档作用: 工程总览 (价值主张 / 使用 / 架构 / 结构); MUST NOT 写发布流程 (→ workflow.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 章节按需增删, 只留项目真有的; 首行一行价值主张, MUST NOT 带 LLM 提示
- 短并列项用表格; 可执行步骤 fenced + `#` 注释同行
- NEVER 写「开发」段 (VibeCoding 不向人类解释 dev 命令)
```

# jj-agentic-launcher

启动器: 把共享 scene prompt 注入 Claude Code / Codex / agy (Antigravity CLI). Rust 单文件可执行 (仅 macOS). 打 tag → GitHub Actions 自动构建并发布 release; 用户用 `install.sh` 一键安装, 或通过内置 `update` 子命令自更新.

## 安装

```bash
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-agentic-launcher/main/scripts/install.sh | bash
```

依赖: `claude` / `codex` / `agy` CLI 按需另行安装并在 `PATH` 中 (只用哪个装哪个). 默认装到 `$HOME/.local/bin`. 可用 `VERSION` / `INSTALL_DIR` / `REPO` 覆写.

## 用法

```
jj-agentic-launcher [scene]                          # Interactive REPL
jj-agentic-launcher [scene] 'prompt'                 # Single-shot + stream-JSON 渲染 (默认)
jj-agentic-launcher -p [scene] 'prompt'              # Single-shot, raw print 透传
jj-agentic-launcher --loop N [scene] 'prompt'        # 同一 single-shot 串行 N 次
jj-agentic-launcher --loop relay [scene] 'prompt'    # 接力式: 后一轮接住前一轮 next_actions
jj-agentic-launcher --loop refine [scene] 'prompt'   # 打磨式: 每轮零上下文重跑原始 prompt
jj-agentic-launcher --pre '<cmd>' [scene] 'prompt'   # 先跑 <cmd>, 引擎继承其 shell 状态
```

- 引擎前缀: 无前缀 = Claude Code, `.` = Codex, `,` = agy — `d` / `.d` / `,d`. 见 [引擎](#引擎).
- 内置 scene: `default` / `ai-expert` / `it-expert` / `code-expert` / `address`.
- 别名: `d`→`default`, `ai`→`ai-expert`, `it`→`it-expert`, `code`→`code-expert`.
- scene 参数可省略 → 走 `scenes.default`, 见 [默认 scene](#默认-scene).

### 引擎

<!-- prettier-ignore -->
| 引擎 | 前缀 | scene 注入方式 |
| --- | --- | --- |
| Claude Code | 无 | `--append-system-prompt` |
| Codex | `.` | `-c developer_instructions=` |
| agy | `,` | prompt 内 `<system_instructions>` 包裹 |

agy (Antigravity CLI) 没有任何 system prompt 参数 (`agy --help` / [headless docs](https://antigravity.google/docs/cli/headless)), 故 scene 只能走 prompt 文本:

- 单次执行: `<system_instructions>scene</system_instructions>` + 原始 prompt, 一并作为 `-p` 的值.
- REPL (无 prompt): scene 作为 `-i` 首轮消息注入 (要求 agent 只回一行「就位」再等指令) — 否则无处绑定 scene.
- 流式渲染下 agy 的 `text_delta` 按字节切片, CJK 字符跨切片会被它自己替换成 `U+FFFD`; 要完整文本用 `-p` (raw text, 无此问题).
- `--loop relay` / `--loop refine` / `<<>>` / `--pre` 全部照常可用.

### 默认 scene

`config.json` → `scenes.default` = 一个 scene token, 语义与命令行参数完全一致: 别名 (`it`) / scene 文件名 (`it-expert`) / 引擎前缀改默认引擎 (`.it` = Codex, `,it` = agy). 改完立即生效, 无需重装.

```json
{ "scenes": { "default": "it", "aliases": { "it": "it-expert" } } }
```

省略 scene 的全部形态 (以 `default: "it"` 为例):

```bash
jj-agentic-launcher                     # REPL, it-expert + Claude
jj-agentic-launcher 'prompt'            # 单参数且不是已知 scene → 当 prompt, it-expert
jj-agentic-launcher -p 'prompt'         # -p / --loop 已隐含 prompt, 单参数必为 prompt
jj-agentic-launcher --loop 3 'prompt'
jj-agentic-launcher '' 'prompt'         # 空 token = 省略
jj-agentic-launcher . 'prompt'          # 裸 . = 默认 scene 强制走 Codex (裸 , = agy)
```

- 两参数时第一个 MUST 是 scene: 未知名字直接报错, NEVER 当 prompt.
- 单参数解析顺序 scene 优先: `jj-agentic-launcher it` = 进 it-expert REPL, 不是跑 prompt "it"; 落到 prompt 分支时 stderr 打 `[info]` 说明 (scene 名打错也能立刻看见).
- `scenes.default` 值为空 / 未知 → `[warn]` + 回退内置 `default`, 不阻断启动.
- `jj-agentic-launcher help` 顶部展示当前生效的默认 scene + 引擎.

### 前置命令 `--pre`

`--pre '<cmd>'` 在交互 `$SHELL` 里执行 `<cmd>`, 随后引擎 `exec` 顶替该 shell 进程 — 继承 `<cmd>` 留下的全部 shell 状态 (cwd / 环境变量 / source / shell function):

```bash
jj-agentic-launcher --pre 'j api' it '讲下这个项目的架构'
jj-agentic-launcher --pre 'cd $(fd -t d | fzf)' d 'review 这个目录'
jj-agentic-launcher --pre 'source .venv/bin/activate && cd backend' code 'run tests'
jj-agentic-launcher --pre 'git pull' --loop refine code 'fix all type errors'
```

- 交互 shell (`-i`) 加载 rc, 所以 `j` / `z` 这类 shell function 可用; fzf 等 TUI 走 `/dev/tty`, 不受 stdout 管道影响.
- `exec` 顶替而非嵌套: 进程树不多一层, 信号与 exit code 直传引擎.
- `<cmd>` 的 stdout 转 stderr, 不污染引擎输出流; 非零退出直接终止, 不启动引擎.
- 每次子进程启动前都执行 (含 `--loop` 每轮、`<<>>` 每段). shell 状态不能跨进程存活, 只跑首轮会让第 2..N 轮回到原 cwd.
- POSIX sh 语法, `$SHELL` 需为 `sh` / `bash` / `zsh`; fish 不支持.
- 仅命令行传入, config.json 不支持 — 可分享的配置文件不承载 shell 命令.

### Prompt 传参

prompt 是单个位置参数, 用 shell 引号 (推荐单引号) 一行喂入. POSIX 单引号内除 `'` 外所有字符 (含换行) 字面保留, 零转义:

```bash
jj-agentic-launcher d 'hello'
jj-agentic-launcher d '多行 prompt
含 $variable、"双引号"、反斜杠 \、特殊符号 ¥%&* 一概原样'
jj-agentic-launcher d "$(cat prompt.md)"     # 文件喂入 (shell 处理)
jj-agentic-launcher -p code 'review 这段 diff'
```

内容含 `'` 时:

```bash
jj-agentic-launcher d "I'm here"             # 切双引号
jj-agentic-launcher d 'I'\''m here'          # POSIX 拼接
jj-agentic-launcher d <<<"I'm here"          # here-string (bash/zsh)
```

### 顺序分段 `<<>>`

prompt 中嵌入 `<<>>` 拆成 N 段独立 single-shot 依序串行执行 (每段全新 child, 零跨轮状态):

```bash
jj-agentic-launcher d 'step 1 <<>> step 2 <<>> step 3'
jj-agentic-launcher -p code 'review src/foo.ts <<>> review src/bar.ts'
```

- 分隔符两侧空白吃掉; 至少 2 段且每段非空.
- 与 `--loop` 互斥.
- 任段失败 warn-continue, 跑满全部段, 返回最后一段 exit code.

### 循环执行

仅非交互场景 (给定 prompt 时) 可用. 等上一次 child 退出再启下一次. 任一轮 child 非 0 退出或 spawn 异常仅 `[warn]` 并继续, loop 必跑满 N 次, 返回最后一轮 exit code.

```bash
jj-agentic-launcher d 'hi' --loop 3
jj-agentic-launcher -p code 'review' --loop 5
```

### 自决循环 `--loop relay` / `--loop refine`

两种模式都让 agent 自决何时停止: 每轮全新独立 child (零历史), 用 handoff JSON 作跨轮信号. 区别在**跨轮带什么**.

<!-- prettier-ignore -->
| 维度 | `--loop relay` (接力) | `--loop refine` (打磨) |
| --- | --- | --- |
| 跨轮带 | next_actions + summary + blockers | 只读 status (end/continue) |
| 第 N 轮看到 | `<previous_handoff>` + `<original_task>` | 与第 1 轮完全相同的原始 prompt |
| 任务关系 | 后一轮**接住**前一轮的子任务 | 后一轮**重做**同一个 prompt |
| status 偏向 | continue (有 next_actions 就 continue) | end (本轮做完就该 end) |
| 适用场景 | 多阶段任务推进 (翻译 + 提 PR、修 bug 组) | 同一 prompt 反复打磨 (性能优化、refactor 试验) |

```bash
# 接力式
jj-agentic-launcher --loop relay d '把 README 翻译成英文并提交 PR'
jj-agentic-launcher --loop relay code 'fix all type errors' --max-iter 50

# 打磨式
jj-agentic-launcher --loop refine d '对整个项目做一次全面性能优化, 找出所有可优化点并修复'
jj-agentic-launcher --loop refine code 'review src/ 找出所有可读性问题并修复' --max-iter 10
```

end 门槛:

- `relay`: agent 对本轮 + 整体任务非常满意, 无遗留 next_actions, 才写 end.
- `refine`: agent 对本轮非常满意, 且认为再让零上下文 agent 跑同样 prompt 也找不出更多, 才写 end.

handoff 形态 (agent 输出, 父进程消费):

```
# --loop relay (接力式)
<<JJ_HANDOFF>>
{
  "status": "end" | "continue",
  "iteration": <number>,
  "summary": "本轮做了什么 (≤80字)",
  "next_actions": ["下一轮 agent 要做的事 1", "..."],
  "blockers": []
}
<<JJ_HANDOFF_END>>

# --loop refine (打磨式) — 极简, 跨轮信号只剩一个 bit
<<JJ_HANDOFF>>
{"status": "end" | "continue"}
<<JJ_HANDOFF_END>>
```

启动时 stderr 打印本地观察端点:

```
==> --loop refine (max 100) — state: http://127.0.0.1:53811/handoff
```

```bash
curl http://127.0.0.1:53811/handoff   # 返回 mode + iteration + history JSON
```

端口 OS 自动分配 (`port: 0`), 每次启动都不同, 进程结束自动关闭, 零文件落盘.

终止条件 / exit code (两模式共用):

<!-- prettier-ignore -->
| 情况 | exit |
| --- | --- |
| handoff.status="end" | 0 |
| 达到 `--max-iter` 上限 | 0 (stderr 警告) |
| 子进程非 0 退出 | 透传该 code |
| 连续 3 轮 handoff 解析失败 | 3 |

## 配置

首次运行自动初始化 `~/.config/jj-agentic-launcher/`:

```
config.json    # 引擎参数 (claude/codex/agy args + interactive/print/stream 分模式覆写) + scene 别名 + 默认 scene
scenes/*.md    # 自定义 scene 文件 (首次运行内置 scene 落盘)
```

- 新增 scene: `scenes/foo.md` + `config.json` → `scenes.aliases` 加 `"f": "foo"` → `jj-agentic-launcher foo` / `f` / `.f` / `,f` 均可用.
- config.json 里整段缺失的引擎 (如老版本写的文件没有 `agy` 段) 用内置默认参数; 写成 `"agy": {}` 才是「不带参数」.
- 引擎自身的模型 / 推理档位不写死在本项目: 需要就往对应引擎段的 `args` 里加 (如 agy 的 `--model` / `--effort`), 或命令行 `-- --model ...` 透传.

## 自更新 / 卸载

```bash
jj-agentic-launcher update      # 与 upgrade 等价, 拉 latest release 原子替换
jj-agentic-launcher uninstall   # 删除当前二进制
```

## 架构

Rust, `cargo build --release` 编译单文件二进制 (darwin arm64/x64). GitHub Actions 于 `v*` tag 触发双架构构建 + 生成 `checksums.txt` + 创建 Release. 运行时依赖: `claude` / `codex` / `agy` (PATH, 按需), `curl` (仅 `update` 子命令). crate 依赖: `serde` / `serde_json` / `sha2`.

## 项目结构

```
src/          # CLI 主体: main / parse / config / run / handoff / server / scene 解析 / 流事件格式化 / update
scenes/       # 内置 scene prompt (compile-time include_str! 嵌入, 首次运行落盘到 ~/.config/)
scripts/      # 辅助脚本
Cargo.toml    # 包定义, VERSION 经 env!(CARGO_PKG_VERSION) 注入二进制
```

子命令: `help` / `-h` / `--help`, `version` / `-v` / `--version`, `update` / `upgrade`, `uninstall`.
