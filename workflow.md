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
bun run start [scene] 'prompt'   # 源码运行 (update/uninstall 子命令被守卫拒绝)
bun run build                    # 编译双架构 → dist/jj-prompt-launcher-darwin-{arm64,x64}
./dist/jj-prompt-launcher-darwin-arm64 version   # 验证构建物
bun run typecheck                # tsc --noEmit
```

# 发布

代码变更完成后立即执行 (= 需求交付的最后环节). 推 `v*` tag → `.github/workflows/release.yml` 触发构建 + `checksums.txt` + Release.

## TL;DR

依序执行:

1. 验证: `bun run typecheck && bun run build && ./dist/jj-prompt-launcher-darwin-arm64 version`
2. 写版本: `package.json#version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致, tag 含 `v` version 不含)
3. 发布: commit + annotated tag (`-a -m`) + push branch + tag
4. 修上版 bug: amend + 删远程 tag + 重打 + force push

## 1. 验证

```bash
bun run typecheck
bun run build
./dist/jj-prompt-launcher-darwin-arm64 version
```

## 2. 写版本

- 版本号: 默认递增 PATCH (第三位); 新功能 → MINOR; 不兼容改动 → MAJOR.
- `package.json#version` + `CHANGELOG.md` + `CHANGELOG.dev.md` 同步编辑 (与 tag 一致, tag 含 `v` version 不含).
- `package.json#version` 经 `build.ts` 通过 `--define BUILD_VERSION` 注入二进制. Actions 第一步会校验 `v${pkg_version} == tag`, 不一致直接 fail.
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

## 4. 修上版 bug

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
