# 部署流程

AI 改完代码主动执行. push `v*` tag 触发 Actions 构建发布.

## 1. 验证

```bash
bun run typecheck
bun run build
./dist/jj-prompt-launcher-darwin-arm64 version
```

## 2. 写版本

- 版本号: 默认递增 PATCH (第三位); 新功能 → MINOR; 不兼容改动 → MAJOR.
- `CHANGELOG.md` 顶部新增 `## [X.Y.Z] - YYYY-MM-DD` 段并列改动, 底部补 `[X.Y.Z]:` 对比链接.
- **CHANGELOG 条目硬约束**: 每条一行, 只写"做了什么 + 关键 flag / 文件 / 行为". 不写设计理由 / 实现细节 / "为什么这么选" / 跨版本引用 (如"思路同 vX"). 这些属于 commit message — commit 详情由 Actions `generate_release_notes` 自动汇总到 Release, 不进 CHANGELOG.
- `package.json#version` 与 tag 一致 (tag 含 `v`, version 不含, 经 `build.ts` 注入二进制). Actions 第一步会校验, 不一致直接 fail.

## 3. 发布

```bash
git add .
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin <branch> vX.Y.Z
```

> 用 annotated tag (`-a -m`) 而非 lightweight: 兼容 `tag.gpgsign=true` 配置 (开启时 lightweight tag 会被强制升级为 signed 但缺 message → fail).

## 4. amend 修上版 bug

AI 自主识别 "刚发版的 bug, 不发新版" 场景 (信号: 反馈指向刚 push 的 tag / 改动极小仅修缺陷 / 语气暗示是上版延续如 "刚那个" "刚发的"). 此时:

> **commit + tag 必须同步更新**: amend 后 commit hash 变了, 远程 tag 仍指向旧 hash → Release artifact 与 main HEAD 分离. 只 force push commit 不够, 必须删远程 tag 后重打, 否则 Actions 不会重跑构建.

```bash
git commit -a --amend --no-edit
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag -a vX.Y.Z -m "vX.Y.Z"
git push --force-with-lease origin <branch>
git push origin vX.Y.Z
```
