# jj-prompt-launcher

`jj-prompt-launcher` 启动器: 把共享 scene prompt 注入 Claude Code / Codex. Bun 单文件可执行 (仅 macOS). 打 tag → GitHub Actions 自动构建并发布 release;用户用 `install.sh` 一键安装, 或通过内置 `update` 子命令自更新.

## 安装

```bash
curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-prompt-launcher/main/install.sh | bash
```

依赖: `claude`、`codex` CLI 需另行安装并在 `PATH` 中.

默认装到 `$HOME/.local/bin`. 可用 `VERSION` / `INSTALL_DIR` / `REPO` 覆写.

## 用法

```
jj-prompt-launcher [scene]                          Interactive REPL
jj-prompt-launcher [scene] 'prompt'                 Single-shot + stream-JSON renderer (default)
jj-prompt-launcher -p [scene] 'prompt'              Single-shot, raw print passthrough
jj-prompt-launcher --loop N [scene] 'prompt'        Run the same single-shot N times serially
jj-prompt-launcher --loop auto [scene] 'prompt'     Relay loop: next turn picks up previous handoff's
                                            next_actions; agent chains work across turns
jj-prompt-launcher --loop refine [scene] 'prompt'   Refine loop: every turn re-runs the ORIGINAL prompt
                                            in a fresh agent (no carry-over except end/continue)
```

- 默认引擎 Claude Code: `jj-prompt-launcher d`. 前缀 `.` 走 Codex: `jj-prompt-launcher .d`.
- 无 scene 参数 → 使用 `scenes.default` (config).
- 内置 scene: `default` / `ai-expert` / `it-expert` / `code-expert` / `address`.
- 别名: `d`→`default`, `ai`→`ai-expert`, `it`→`it-expert`, `code`→`code-expert`.

### Prompt 传参

prompt 是单个位置参数, 用 shell 引号 (推荐单引号) 一行喂入. POSIX 单引号内除 `'` 外所有字符 (含换行) 字面保留, **零转义**:

```bash
jj-prompt-launcher d 'hello'

jj-prompt-launcher d '多行 prompt
含 $variable、"双引号"、反斜杠 \、特殊符号 ¥%&* 一概原样'

jj-prompt-launcher d "$(cat prompt.md)"     # 文件喂入 (shell 处理, jj-prompt-launcher 不需要 -f)
jj-prompt-launcher -p code 'review 这段 diff'   # 切回 raw print 模式 (不渲染 stream-JSON)

# 内容含 ' 时三种应对 (仍是单行):
jj-prompt-launcher d "I'm here"             # 切双引号
jj-prompt-launcher d 'I'\''m here'          # POSIX 经典拼接
jj-prompt-launcher d <<<"I'm here"          # here-string (bash/zsh)
```

### 顺序分段 (`<<>>`)

在 prompt 中嵌入 `<<>>` 即把它拆成 N 段独立 single-shot, 依序串行执行 (每段一个全新 child, 零跨轮状态). 无需额外 flag:

```bash
jj-prompt-launcher d 'step 1 <<>> step 2 <<>> step 3'
jj-prompt-launcher -p code 'review src/foo.ts <<>> review src/bar.ts'
```

- 分隔符两侧空白会被吃掉.
- 至少 2 段且每段非空.
- 与 `--loop` 互斥 (语义复合度过高, 单段需要重跑请去掉 `<<>>`).
- 稳定性同 fixed loop: 任段失败 warn-continue, 跑满全部段, 返回最后一段 exit code.

### 循环执行

仅非交互场景 (即同时给定 prompt 时) 可用. 等上一次 child 退出再启下一次. 任一轮 child 非 0 退出或 spawn 异常都仅打 `[warn]` 并继续下一轮, loop 必跑满 N 次, 返回最后一轮 exit code.

```bash
jj-prompt-launcher d 'hi' --loop 3          # 串行跑 3 次 (默认 stream)
jj-prompt-launcher -p code 'review' --loop 5   # 切回 print 再循环
```

### 自动循环 (`--loop auto` / `--loop refine`)

两种模式都让 agent 自己决定何时停止: 每轮跑一个**全新独立 child** (零历史), 用 handoff JSON 作为跨轮信号. 区别在于**跨轮带什么**.

| 维度 | `--loop auto` (接力) | `--loop refine` (打磨) |
| --- | --- | --- |
| 跨轮带 | next_actions + summary + blockers | 只读 status (end/continue) |
| 第 N 轮看到 | `<previous_handoff>` + `<original_task>` | 与第 1 轮完全相同的原始 prompt |
| 任务关系 | 后一轮**接住**前一轮的子任务 | 后一轮**重做**同一个 prompt |
| status 偏向 | continue (有 next_actions 就 continue) | end (本轮做完就该 end) |
| 适用场景 | 多阶段任务推进 (翻译 + 提 PR、修一组 bug 等) | 同一 prompt 反复打磨 (大项目优化、refactor 试验等) |

```bash
# 接力式: 第一轮拆任务, 后续轮逐项推进
jj-prompt-launcher --loop auto d '把 README 翻译成英文并提交 PR'
jj-prompt-launcher --loop auto code 'fix all type errors' --max-iter 50

# 打磨式: 同一 prompt 每轮换全新视角重做
jj-prompt-launcher --loop refine d '对整个项目做一次全面性能优化, 找出所有可优化点并修复'
jj-prompt-launcher --loop refine code 'review src/ 找出所有可读性问题并修复' --max-iter 10
```

end 门槛 (两种模式都有, 措辞略不同):

- `auto`: agent 对本轮**+** 整体任务非常满意, 无遗留 next_actions, 才写 end
- `refine`: agent 对本轮非常满意, **且**认为再让一个零上下文 agent 跑同样的 prompt 也找不出更多, 才写 end

handoff 形态 (agent 输出, 父进程消费):

```
# --loop auto (接力式)
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

启动时 stderr 会打印一个本地观察端点, 形如:

```
==> --loop refine (max 100) — state: http://127.0.0.1:53811/handoff
```

curl 时**照抄那一行的 URL**:

```bash
curl http://127.0.0.1:53811/handoff   # 返回 mode + iteration + history JSON
```

端口由 OS 自动分配 (`port: 0`), 每次启动都不同, 进程结束自动关闭, 零文件落盘.

终止条件 | exit code (两模式共用):

| 情况 | exit |
| --- | --- |
| handoff.status="end" | 0 |
| 达到 `--max-iter` 上限 | 0 (stderr 警告) |
| 子进程非 0 退出 | 透传该 code |
| 连续 3 轮 handoff 解析失败 | 3 |

## 配置

首次运行自动初始化 `~/.config/jj-prompt-launcher/`:

```
config.json    引擎参数 (claude/codex args + interactive/print/stream 分模式覆写) + scene 别名
scenes/*.md    自定义 scene 文件 (首次运行内置 scene 落盘)
```

新增 scene: 在 `scenes/` 放 `foo.md`, 可在 `config.json` → `scenes.aliases` 加 `"f": "foo"`, 之后 `jj-prompt-launcher foo` / `jj-prompt-launcher f` / `jj-prompt-launcher .f` 均可用.

## 自更新 / 卸载

```bash
jj-prompt-launcher update      # 与 upgrade 等价, 拉取 latest release 并原子替换
jj-prompt-launcher uninstall   # 删除当前二进制
```

## 开发

```bash
bun install
bun run start                 # 源码运行 (update/uninstall 子命令会被守卫拒绝)
bun run build                 # 编译双架构 → dist/jj-prompt-launcher-darwin-{arm64,x64}
bun run typecheck
```

子命令: `help` / `-h` / `--help`, `version` / `-v` / `--version`, `update` / `upgrade`, `uninstall`.

## 发布

推 `v*` tag 即触发 `.github/workflows/release.yml`, 构建 + 生成 `checksums.txt` + 创建 Release. 详见 [deploy.md](./deploy.md).

```bash
git tag -a v0.3.0 -m "v0.3.0"
git push origin v0.3.0
```
