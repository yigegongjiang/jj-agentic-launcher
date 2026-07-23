```When Editing
本文档作用: 工程工作流程 (可用工具 / 调试 / 发布); MUST NOT 写工程说明 (→ README.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 所有段落均为条件段, 根据工程实际决定保留或删除; 存在即为明确流程, MUST NOT 附加强度标记
- 发布内按顺序编号步骤; 顶部 TL;DR ≤ 5 行; 删除子段后重编号保持连续
- 风险点 / 不可逆操作用 `>` 引用块; 高危操作 MUST 标禁用条件
```

# 可用工具

- `gh` 已登录

# 调试

CLI 项目, 无 dev server. 源码直接跑或先编译再跑构建物.

```bash
cargo run -- [scene] 'prompt'    # 源码运行
cargo build --release            # 编译当前架构 → target/release/jj-prompt-launcher
cargo build --release --target x86_64-apple-darwin   # 交叉编译另一架构 (先 rustup target add)
./target/release/jj-prompt-launcher version          # 验证构建物
cargo build                      # 类型 / 借用检查即编译 (无独立 typecheck)
```

> update/uninstall 守卫仅比对二进制 basename == `jj-prompt-launcher`. `cargo run` 下 basename 恰为该名 → 守卫放行 (作用于 target/ 构建物); dev 勿 `cargo run -- uninstall/update`.

# 发布

代码变更完成后立即执行 (= 需求交付的最后环节). 推 `v*` tag → `.github/workflows/release.yml` 触发构建 + `checksums.txt` + Release.

## TL;DR

依序执行:

1. 验证: `cargo build --release && cargo build --release --target x86_64-apple-darwin && ./target/release/jj-prompt-launcher version`
2. 写版本: `Cargo.toml [package] version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致, tag 含 `v` version 不含); `cargo build` 一次让 `Cargo.lock` 同步, 一并提交
3. 发布: commit + annotated tag (`-a -m`) + push branch + tag
4. 本机自安装: `bash scripts/install-local.sh` (编译 native 产物装到 `~/.local/bin`, 本机即刻用上新版, 不等 Actions)
5. 修上版 bug: amend + 删远程 tag + 重打 + force push

## 1. 验证

```bash
cargo build --release
cargo build --release --target x86_64-apple-darwin
./target/release/jj-prompt-launcher version
```

## 2. 写版本

- 版本号: 默认递增 PATCH (第三位); 新功能 → MINOR; 不兼容改动 → MAJOR.
- `Cargo.toml [package] version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致, tag 含 `v` version 不含).
- version 经 `env!(CARGO_PKG_VERSION)` 注入二进制. Actions 第一步会校验 `v${cargo_version} == tag`, 不一致直接 fail.
- 改 version 后 `cargo build` 一次让 `Cargo.lock` 同步, 与源码一并提交.
- CHANGELOG.md 顶部新增 `## [X.Y.Z] - YYYY-MM-DD` 段, 底部补 `[X.Y.Z]:` 对比链接; CHANGELOG.dev.md 同步镜像 + 技术子项.

## 3. 发布

```bash
git add .
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main
git push origin vX.Y.Z
```

> 用 annotated tag (`-a -m`) 而非 lightweight: 兼容 `tag.gpgsign=true` 配置 (开启时 lightweight tag 会被强制升级为 signed 但缺 message → fail).

## 4. 本机自安装

推 tag 后本机即刻装上新版 (native 产物), 不必等 Actions 构建 + 下载.

```bash
bash scripts/install-local.sh   # cargo build --release → 装到 ~/.local/bin/jj-prompt-launcher
```

> 装的是 native 单架构产物 (与 `update` 拉取的 release asset 内容一致). 复用 `1. 验证` 已编译的 `target/release/`, cargo 增量近乎瞬时.

## 5. 修上版 bug

上版存在明显 bug 时 (信号: 反馈指向刚 push 的 tag / 改动极小仅修缺陷 / "刚发的"), amend 修复后重发同版本号.

> **commit + tag 必须同步更新**: amend 后 commit hash 变了, 远程 tag 仍指向旧 hash → Release artifact 与 main HEAD 分离. 只 force push commit 不够, 必须删远程 tag 后重打, 否则 Actions 不会重跑构建.

```bash
git commit -a --amend --no-edit
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag -a vX.Y.Z -m "vX.Y.Z"
git push --force-with-lease origin main
git push origin vX.Y.Z
```
