import { createHash } from "node:crypto";
import { chmod, mkdir, rename, unlink, writeFile } from "node:fs/promises";
import { basename, dirname, join } from "node:path";

import { downloadWithProgress } from "./download";
import { DEFAULT_AUTO_MAX_ITER, parseInvocation, UsageError, type Invocation, type LoopSpec } from "./parse";
import { runInvocation } from "./run";
import { listAllSceneNames } from "./scenes";
import { getConfigDir } from "./config";
import { ensureInitialized } from "./init";
import {
  buildContinuePrompt,
  buildProtocolPrompt,
  buildRefineProtocolPrompt,
  createAutoLoopState,
  parseHandoff,
  snapshotState,
} from "./handoff";
import pkg from "../package.json" with { type: "json" };

// `build.ts` injects BUILD_* via `--define` at compile time.
// In dev (`bun run start`), they are undeclared; `typeof` is safe and falls back to package.json.
declare const BUILD_NAME: string | undefined;
declare const BUILD_VERSION: string | undefined;
declare const BUILD_REPO: string | undefined;

const NAME = typeof BUILD_NAME === "string" ? BUILD_NAME : pkg.name;
const VERSION = typeof BUILD_VERSION === "string" ? BUILD_VERSION : pkg.version;
const REPO = typeof BUILD_REPO === "string" ? BUILD_REPO : (pkg.repository ?? "");

function buildHelpText(): string {
  const scenes = listAllSceneNames();
  const configDir = getConfigDir();

  return `${NAME} ${VERSION} — Launch Claude Code or Codex with shared scene prompts

Usage:
  ${NAME} [scene]                       Interactive REPL
  ${NAME} [scene] 'prompt'              Single-shot run with stream-JSON renderer (default)
  ${NAME} -p [scene] 'prompt'           Single-shot run with raw print passthrough
  ${NAME} --loop N [scene] 'prompt'       Run the same single-shot N times serially
  ${NAME} --loop auto [scene] 'prompt'    Relay loop: each turn picks up previous turn's handoff
                                          (status/next_actions). Stops on status="end" or --max-iter
  ${NAME} --loop refine [scene] 'prompt'  Refine loop: each turn runs the ORIGINAL prompt verbatim
                                          in a fresh agent (no cross-turn carry-over except the
                                          end/continue signal). Stops on status="end" or --max-iter

Auto-loop options:
  --max-iter N                            Safety cap for --loop auto / --loop refine
                                          (default ${DEFAULT_AUTO_MAX_ITER})

Prompt is a single positional argument. Use shell quoting for any complexity:
  ${NAME} d 'multi-line
prompt with $vars, "quotes", \\ backslashes — POSIX single-quote keeps it literal'

Sequential prompts:
  Embed \`<<>>\` to split one prompt into N independent single-shots, run in order:
    ${NAME} d 'step 1 <<>> step 2 <<>> step 3'
  Cannot combine with --loop (each segment runs exactly once).

Passthrough to claude/codex:
  Everything after \`--\` is forwarded verbatim to the underlying engine,
  appended after scene injection and before the prompt (no validation):
    ${NAME} code 'fix the bug' -- --model opus --add-dir ../shared
    ${NAME} .d -- --search          (Codex flag, REPL with no prompt)

Scenes:
  ${scenes.join(", ")}
  Default is Claude Code, use . prefix for Codex, e.g. .d / .code

Meta commands:
  help, --help, -h            Show this help message
  version, --version, -v      Show version information
  update, upgrade             Download the latest release and replace this binary
  uninstall                   Remove this binary from disk

Config:
  ${configDir}/config.json    Launch arguments
  ${configDir}/scenes/*.md    Custom scenes
`;
}

function detectAsset(): string {
  if (process.platform !== "darwin") {
    throw new Error(`unsupported OS: ${process.platform} (only darwin is supported)`);
  }
  const a = process.arch;
  if (a !== "x64" && a !== "arm64") throw new Error(`unsupported arch: ${a}`);
  return `${NAME}-darwin-${a}`;
}

async function update(): Promise<number> {
  const assetName = detectAsset();
  const base = `https://github.com/${REPO}/releases/latest/download`;
  const assetUrl = `${base}/${assetName}`;
  const checksumsUrl = `${base}/checksums.txt`;
  const dest = process.execPath;
  if (basename(dest) !== NAME) {
    throw new Error(
      `refusing to self-update: current executable is "${basename(dest)}", expected "${NAME}". ` +
        `self-update only works on the installed binary, not when running from source via bun.`,
    );
  }

  console.log(`==> Updating ${NAME}`);
  console.log(`    repo:   ${REPO}`);
  console.log(`    target: ${dest}`);
  console.log(`    before: ${NAME} ${VERSION}`);

  console.log(`==> Downloading ${assetUrl}`);
  const assetBytes = await downloadWithProgress(assetUrl);

  // Verify checksum if checksums.txt exists for this release.
  try {
    const checksumsRes = await fetch(checksumsUrl, { redirect: "follow" });
    if (checksumsRes.ok) {
      const text = await checksumsRes.text();
      const line = text.split(/\r?\n/).find((l) => l.trim().endsWith(` ${assetName}`));
      if (line) {
        const expected = line.trim().split(/\s+/)[0]!.toLowerCase();
        const actual = createHash("sha256").update(assetBytes).digest("hex");
        if (expected !== actual) {
          console.error(`error: checksum mismatch (expected ${expected}, got ${actual})`);
          return 1;
        }
        console.log("==> Checksum OK");
      }
    }
  } catch {
    // Checksums are best-effort.
  }

  // Atomic replace via tmp on the same filesystem.
  await mkdir(dirname(dest), { recursive: true });
  const tmp = join(dirname(dest), `.${NAME}.update.${process.pid}`);
  await writeFile(tmp, assetBytes);
  await chmod(tmp, 0o755);
  try {
    await rename(tmp, dest);
  } catch (err: unknown) {
    await unlink(tmp).catch(() => {});
    throw err;
  }

  console.log(`==> Updated: ${dest}`);
  try {
    const r = Bun.spawnSync([dest, "version"]);
    if (r.success && r.stdout) {
      const after = new TextDecoder().decode(r.stdout).trim();
      if (after) console.log(`    after:  ${after}`);
    }
  } catch {
    // best-effort; if the new binary cannot exec, the replace itself already succeeded.
  }
  return 0;
}

async function uninstall(): Promise<number> {
  const dest = process.execPath;
  if (basename(dest) !== NAME) {
    throw new Error(
      `refusing to uninstall: current executable is "${basename(dest)}", expected "${NAME}". ` +
        `uninstall only works on the installed binary, not when running from source via bun.`,
    );
  }

  console.log(`==> Uninstalling ${NAME}`);
  console.log(`    target: ${dest}`);

  await unlink(dest);

  console.log(`==> Removed: ${dest}`);
  return 0;
}

async function handleMetaCommand(arg: string | undefined): Promise<number | null> {
  switch (arg) {
    case "help":
    case "--help":
    case "-h":
      process.stdout.write(buildHelpText());
      return 0;
    case "version":
    case "--version":
    case "-v":
      process.stdout.write(`${NAME} ${VERSION}\n`);
      return 0;
    case "update":
    case "upgrade":
      try {
        return await update();
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        console.error(`error: update failed: ${msg}`);
        return 1;
      }
    case "uninstall":
      try {
        return await uninstall();
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        console.error(`error: uninstall failed: ${msg}`);
        return 1;
      }
    default:
      return null;
  }
}

function parseFlags(args: string[]): { args: string[]; wantPrint: boolean; loop: LoopSpec; passthrough: string[] } {
  let wantPrint = false;
  let loopValue: string | null = null;
  let maxIter = DEFAULT_AUTO_MAX_ITER;
  let sawMaxIter = false;
  const filtered: string[] = [];
  const passthrough: string[] = [];

  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    // Everything after a literal `--` is forwarded verbatim to the child engine;
    // it bypasses jj-prompt-launcher's own flag parsing entirely.
    if (arg === "--") {
      passthrough.push(...args.slice(i + 1));
      break;
    }
    if (arg === "--print" || arg === "-p" || arg === "print") {
      wantPrint = true;
      continue;
    }
    if (arg === "--loop") {
      const next = args[i + 1];
      if (next === undefined) {
        throw new UsageError('`--loop` requires a value (positive integer, "auto", or "refine").');
      }
      loopValue = next;
      i++;
      continue;
    }
    if (arg === "--max-iter") {
      const next = args[i + 1];
      if (next === undefined) throw new UsageError("`--max-iter` requires a positive integer value.");
      if (!/^\d+$/.test(next) || Number(next) < 1) {
        throw new UsageError(`Invalid --max-iter value "${next}". Expected a positive integer.`);
      }
      maxIter = Number(next);
      sawMaxIter = true;
      i++;
      continue;
    }
    filtered.push(arg);
  }

  let loop: LoopSpec = { kind: "fixed", count: 1 };
  if (loopValue !== null) {
    if (loopValue === "auto") {
      loop = { kind: "auto", maxIter };
    } else if (loopValue === "refine") {
      loop = { kind: "refine", maxIter };
    } else {
      if (!/^\d+$/.test(loopValue) || Number(loopValue) < 1) {
        throw new UsageError(
          `Invalid --loop value "${loopValue}". Expected a positive integer, "auto", or "refine".`,
        );
      }
      if (sawMaxIter) {
        throw new UsageError("`--max-iter` only applies to `--loop auto` / `--loop refine`.");
      }
      loop = { kind: "fixed", count: Number(loopValue) };
    }
  } else if (sawMaxIter) {
    throw new UsageError("`--max-iter` requires `--loop auto` or `--loop refine`.");
  }

  return { args: filtered, wantPrint, loop, passthrough };
}

function getRawArgs(argv: string[]): string[] {
  if (argv.length <= 1) {
    return [];
  }

  const second = argv[1];
  if (looksLikeScriptPath(second)) {
    return argv.slice(2);
  }

  return argv.slice(1);
}

function looksLikeScriptPath(value: string | undefined): boolean {
  if (!value || value.startsWith("-")) {
    return false;
  }

  if (value.startsWith("/$bunfs/")) {
    return true;
  }

  return /\.(?:[cm]?[jt]sx?)$/i.test(value);
}

// Stability rule for all loop modes: a single iteration never aborts the loop.
// child exit !=0, spawn errors, stream errors, handoff parse failures — all are
// logged as `[warn]` and the loop proceeds. Only `--max-iter` (auto/refine) or the
// configured count (fixed) terminates the loop. status="end" stops auto/refine early.

// Sequential split: prompt contains `<<>>` → split into N independent single-shots,
// each run as if a separate `jj-prompt-launcher` invocation. Same stability rule: a step
// failure warns and the next step still runs; returns the last step's exit code.
async function runSerialLoop(invocation: Invocation, prompts: string[]): Promise<number> {
  let lastExit = 0;
  const total = prompts.length;
  for (let i = 0; i < total; i++) {
    const prompt = prompts[i]!;
    process.stderr.write(`==> step ${i + 1}/${total}\n`);
    try {
      lastExit = await runInvocation(invocation, { promptOverride: prompt });
      if (lastExit !== 0) {
        process.stderr.write(
          `[warn] step ${i + 1}/${total}: child exited with code ${lastExit}${i + 1 < total ? "; proceeding to next step." : "."}\n`,
        );
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      lastExit = 1;
      process.stderr.write(
        `[warn] step ${i + 1}/${total}: runInvocation threw: ${msg}${i + 1 < total ? "; proceeding to next step." : "."}\n`,
      );
    }
  }
  return lastExit;
}

async function runFixedLoop(invocation: Invocation, count: number): Promise<number> {
  let lastExit = 0;
  for (let i = 1; i <= count; i++) {
    if (count > 1) {
      process.stderr.write(`==> loop ${i}/${count}\n`);
    }
    try {
      lastExit = await runInvocation(invocation);
      if (lastExit !== 0) {
        process.stderr.write(
          `[warn] loop ${i}/${count}: child exited with code ${lastExit}${i < count ? "; proceeding to next iteration." : "."}\n`,
        );
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      lastExit = 1;
      process.stderr.write(
        `[warn] loop ${i}/${count}: runInvocation threw: ${msg}${i < count ? "; proceeding to next iteration." : "."}\n`,
      );
    }
  }
  return lastExit;
}

type AgentLoopMode = "auto" | "refine";

async function runAgentLoop(
  invocation: Invocation,
  maxIter: number,
  mode: AgentLoopMode,
): Promise<number> {
  if (!invocation.userText) {
    throw new UsageError(`\`--loop ${mode}\` requires a prompt argument.`);
  }
  const originalPrompt = invocation.userText;
  const state = createAutoLoopState(maxIter);
  const protocolSuffix =
    mode === "auto" ? buildProtocolPrompt(maxIter) : buildRefineProtocolPrompt(maxIter);

  const server = Bun.serve({
    port: 0,
    hostname: "127.0.0.1",
    fetch(req) {
      const url = new URL(req.url);
      if (url.pathname === "/handoff" || url.pathname === "/") {
        return new Response(
          JSON.stringify({ mode, ...snapshotState(state) }, null, 2),
          { headers: { "content-type": "application/json; charset=utf-8" } },
        );
      }
      return new Response("Not Found", { status: 404 });
    },
  });

  process.stderr.write(
    `==> --loop ${mode} (max ${maxIter}) — state: http://${server.hostname}:${server.port}/handoff\n`,
  );

  const stop = () => {
    try {
      server.stop(true);
    } catch {
      // best effort
    }
  };

  try {
    while (true) {
      if (state.stopRequested) break;
      if (state.iteration >= maxIter) {
        process.stderr.write(`==> --loop ${mode}: max-iter ${maxIter} reached, stopping.\n`);
        break;
      }

      state.iteration += 1;
      state.updatedAt = Date.now();
      process.stderr.write(`==> loop ${state.iteration}/${maxIter} (${mode})\n`);

      let accum = "";
      const onAgentText = (t: string) => {
        accum += t;
      };

      // auto: inject previous handoff as <previous_handoff> baton from round 2 onwards.
      // refine: every round uses the original prompt verbatim — zero carry-over.
      const promptOverride =
        mode === "auto" && state.handoff
          ? buildContinuePrompt(originalPrompt, state.handoff)
          : undefined;

      let exit: number;
      try {
        exit = await runInvocation(invocation, {
          onAgentText,
          promptOverride,
          systemSuffix: protocolSuffix,
        });
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        state.lastError = `iteration ${state.iteration} threw: ${msg}`;
        state.updatedAt = Date.now();
        process.stderr.write(
          `[warn] loop ${state.iteration}/${maxIter} (${mode}): runInvocation threw: ${msg}; proceeding to next iteration.\n`,
        );
        continue;
      }

      if (exit !== 0) {
        state.lastError = `iteration ${state.iteration} exited with code ${exit}`;
        state.updatedAt = Date.now();
        process.stderr.write(
          `[warn] loop ${state.iteration}/${maxIter} (${mode}): child exited with code ${exit}; proceeding to next iteration.\n`,
        );
        continue;
      }

      const parsed = parseHandoff(accum);
      if (!parsed) {
        state.parseFailures += 1;
        state.lastError = `no handoff sentinel found in turn ${state.iteration}`;
        state.updatedAt = Date.now();
        const tail = accum.slice(-400).replace(/\n/g, "\\n");
        process.stderr.write(
          `[warn] loop ${state.iteration}/${maxIter} (${mode}): no handoff sentinel (consecutive failures=${state.parseFailures}); proceeding to next iteration. agent_output_tail="${tail}"\n`,
        );
        continue;
      }

      state.parseFailures = 0;
      state.handoff = parsed.handoff;
      state.rawHandoffText = parsed.raw;
      state.history.push(parsed.handoff);
      state.updatedAt = Date.now();
      state.lastError = undefined;

      process.stderr.write(
        `==> handoff status=${parsed.handoff.status}  summary="${parsed.handoff.summary}"\n`,
      );

      if (parsed.handoff.status === "end") {
        state.stopRequested = true;
        break;
      }
    }

    // Surface a non-zero exit if the loop never produced a successful handoff
    // (e.g., every iteration crashed or failed to emit the sentinel). Otherwise
    // a clean run — including max-iter termination after at least one good
    // handoff — returns 0.
    if (state.history.length === 0 && state.lastError) {
      process.stderr.write(
        `[error] --loop ${mode}: no successful handoff across ${state.iteration} iteration(s). lastError=${state.lastError}\n`,
      );
      return 4;
    }
    return 0;
  } finally {
    stop();
  }
}

const rawArgs = getRawArgs(process.argv);

const metaExit = await handleMetaCommand(rawArgs[0]);
if (metaExit !== null) process.exit(metaExit);

ensureInitialized();

try {
  const { args, wantPrint, loop, passthrough } = parseFlags(rawArgs);
  const invocation = parseInvocation(args, wantPrint, loop, passthrough);

  const exitCode = invocation.userTexts
    ? await runSerialLoop(invocation, invocation.userTexts)
    : invocation.loop.kind === "auto"
      ? await runAgentLoop(invocation, invocation.loop.maxIter, "auto")
      : invocation.loop.kind === "refine"
        ? await runAgentLoop(invocation, invocation.loop.maxIter, "refine")
        : await runFixedLoop(invocation, invocation.loop.count);

  process.exit(exitCode);
} catch (error) {
  if (error instanceof UsageError) {
    process.stderr.write(`${error.message}\n\n${buildHelpText()}`);
    process.exit(2);
  }

  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`${message}\n`);
  process.exit(1);
}
