# jj-prompt-launcher

`jj-prompt-launcher` 启动器: 把共享 scene prompt 注入 Claude Code / Codex. Bun 单文件可执行 (仅 macOS). 改完代码 → 在 `CHANGELOG.md` 顶部新增版本段 → 按 [deploy.md](./deploy.md) 发布.

## AI-only 工程声明

本工程为 **AI-only 项目**, 人类不参与开发流程:

1. 代码编写、测试、构建、部署、发布 — 全部由 Claude Code 或 Codex 执行.
2. 设计决策 (架构/技术选型/目录结构/命名/依赖等) 以 Claude Code 或 Codex 的判断为准, **不需要参考人类开发者的惯例/最佳实践**, 除非该惯例本身就是 AI 自己的最优解.
3. 非必要不反问用户, 直接决策并执行 (deploy / 技术抉择 / 文档同步 / 版本号 / changelog 等).
4. 所有文档 (README / CHANGELOG / deploy.md / AGENTS.md / 注释等) 必须**简洁精炼、重点突出、零冗余** — 能一行讲清不写两行, 能用列表不用段落, 不堆砌背景/客套/重复信息. 宁可信息密度过载, 不要废话填充.

简言之: 用户角色 = 触发者 + 验收者, 不是协作开发者. 不要把人类拉进设计回路.

`package.json#version` 经 `build.ts` 注入二进制, 必须与 tag 一致 (Actions 会做一致性校验, 不一致直接 fail).

## 默认 Release 闭环

收到 "改 X / 加 Y / 修 Z" 默认 = 完整 deploy 闭环, **不询问 / 不停顿 / 不只改不发**:

1. 改代码/配置 → `bun run typecheck` + `bun run build` + `./dist/jj-prompt-launcher-darwin-arm64 version` 本地验证.
2. lockstep bump: `package.json#version` + `CHANGELOG.md` 顶部新增 `## [X.Y.Z] - YYYY-MM-DD` 段 + 底部对比链接.
3. `git commit -m "release: vX.Y.Z"` → `git tag -a vX.Y.Z -m "vX.Y.Z"` → `git push origin <branch> vX.Y.Z`.
4. 等 GitHub Actions `Release` workflow success; 失败时读 log 修代码或按 [`deploy.md`](./deploy.md) `amend` 流程修上版, 不卡在 "要不要 deploy / workflow fail 了".

完整命令、版本号规则与 amend 修上版 bug 流程见 [`deploy.md`](./deploy.md).

**豁免发布闭环**: 用户明示 "只改不发 / 先看看 / 本地试"; 或改动仅限文档 (`*.md`)、`scenes/*.md`、注释、`.gitignore`、jjask 记录.

## 命名约定

- `package.json#name` == repo 名 == binary 名 == `jj-prompt-launcher`, 三者一致.
- binary 产物名由 `package.json#name` 派生: `build.ts` 产出 `jj-prompt-launcher-darwin-*`, `install.sh` 的 `BIN_NAME` 与之对齐. 改二进制名须同步这三处.

## 边界

- 仅 macOS (x64 + arm64). 其它平台 `install.sh` 与 `update` 子命令都会主动拒绝.
- `update` / `uninstall` 子命令只在编译后的二进制可用. `bun run start update` 会被守卫拦截 (避免覆盖系统 bun).
- 自更新与 `install.sh` 默认拉 GitHub Release 的 `latest`; checksum 校验是 best-effort (`checksums.txt` 缺失则跳过).
- 不提交 `dist/`、`node_modules/`、`*.bun-build` (已在 `.gitignore`).

## 运行时配置

首次运行自动初始化 `~/.config/jj-prompt-launcher/`:

- `config.json` — 引擎参数 + scene 别名 (`DEFAULT_CONFIG` 见 `src/config.ts`)
- `scenes/*.md` — 内置 scene 文件 (`address` / `ai-expert` / `code-expert` / `default` / `it-expert`), 编译时通过 `import ... with { type: "text" }` 嵌入二进制, 首次运行落盘.

用户后续可直接编辑 `~/.config/jj-prompt-launcher/scenes/` 增删 scene, 工程内置 scene 仅作 seed.
