```When Editing
本文档作用: 工程工作流程 (可用工具 / 调试 / 发布); MUST NOT 写工程说明 (→ README.md) / LLM 约束 (→ AGENTS.md)
遵循 AGENTS.md 文档编写规范
- 所有段落均为条件段, 根据工程实际决定保留或删除; 存在即为明确流程, MUST NOT 附加强度标记
- 发布内按顺序编号步骤; 顶部 TL;DR ≤ 5 行; 删除子段后重编号保持连续
- 风险点 / 不可逆操作用 `>` 引用块; 高危操作 MUST 标禁用条件
```

# 可用工具

- `gh` 已登录

# 发布

代码变更完成后立即执行 (= 需求交付的最后环节). 交付 = 预部署 + push. 推 `v*` tag → `.github/workflows/release.yml` 触发多架构构建 + `checksums.txt` + Release.

## TL;DR

依序执行:

1. 验证: `cargo build --release && cargo build --release --target x86_64-apple-darwin && ./target/release/jj-agentic-launcher version`
2. 写版本: `Cargo.toml [package] version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致); `cargo build` 一次同步 `Cargo.lock`
3. 预部署: `bash scripts/install-local.sh` (本机装上新版)
4. 发布: commit + annotated tag (`-a -m`) + push branch + tag

## 1. 验证

```bash
cargo build --release
cargo build --release --target x86_64-apple-darwin
./target/release/jj-agentic-launcher version
```

> 编译即类型 / 借用检查, 无独立 typecheck. 交叉编译目标需先 `rustup target add x86_64-apple-darwin`.

## 2. 写版本

- 版本号: 默认递增 PATCH (第三位); 超大功能更新 / 调整 → MINOR; 禁止 MAJOR (除非人类主动要求).
- `Cargo.toml [package] version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致, tag 含 `v` version 不含).
- version 经 `env!(CARGO_PKG_VERSION)` 注入二进制; Actions 第一步校验 `v${cargo_version} == tag`, 不一致直接 fail.
- 改 version 后 `cargo build` 一次让 `Cargo.lock` 同步, 与源码一并提交.
- CHANGELOG.md 顶部新增 `## [X.Y.Z] - YYYY-MM-DD` 段, 底部补 `[X.Y.Z]:` 对比链接; CHANGELOG.dev.md 同步镜像 + 技术子项.

## 3. 预部署

本机完成实际交付: 编译 native 产物装到 `~/.local/bin`, 即刻用上新版, 不等 Actions.

```bash
bash scripts/install-local.sh
```

## 4. 发布

```bash
git add .
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main
git push origin vX.Y.Z
```

> 用 annotated tag (`-a -m`) 而非 lightweight: 兼容 `tag.gpgsign=true` 配置 (开启时 lightweight tag 会被强制升级为 signed 但缺 message → fail).
