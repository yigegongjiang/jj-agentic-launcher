import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, join } from "node:path";
import { homedir } from "node:os";

export function getConfigDir(): string {
  const home = process.env.HOME || homedir();
  return join(home, ".config", "jj-prompt-launcher");
}

// --- Types ---

export interface EngineConfig {
  args?: string[];
  interactive?: string[];
  print?: string[];
  stream?: string[];
}

export interface ScenesConfig {
  default?: string;
  aliases?: Record<string, string>;
}

export interface UserConfig {
  claude?: EngineConfig;
  codex?: EngineConfig;
  scenes?: ScenesConfig;
}

// --- Default config: seed data for first-time init ---

export const DEFAULT_CONFIG: UserConfig = {
  claude: {
    args: ["--dangerously-skip-permissions", "--allow-dangerously-skip-permissions"],
    interactive: ["--ide"],
    print: ["-p"],
    stream: ["--output-format", "stream-json", "--verbose", "--include-partial-messages"],
  },
  codex: {
    args: ["--dangerously-bypass-approvals-and-sandbox", "-c", 'web_search="live"'],
    interactive: [],
    print: [],
    stream: ["--json"],
  },
  scenes: {
    default: "default",
    aliases: {
      d: "default",
      ai: "ai-expert",
      code: "code-expert",
      it: "it-expert",
    },
  },
};

// --- Cached state ---

let _config: UserConfig | undefined;
let _userScenes: Map<string, string> | undefined;
let _initialized: boolean | undefined;

export function isInitialized(): boolean {
  if (_initialized !== undefined) return _initialized;
  _initialized = existsSync(join(getConfigDir(), "config.json"));
  return _initialized;
}

export function loadConfig(): UserConfig {
  if (_config !== undefined) return _config;

  const configFile = join(getConfigDir(), "config.json");

  if (!existsSync(configFile)) {
    _config = {};
    return _config;
  }

  try {
    _config = JSON.parse(readFileSync(configFile, "utf-8")) as UserConfig;
  } catch {
    process.stderr.write(`[warn] Failed to parse config ${configFile}, using defaults.\n`);
    _config = {};
  }

  return _config;
}

/**
 * Returns merged args from config with progressive inheritance:
 *   interactive → args + interactive
 *   print       → args + print
 *   stream      → args + print + stream
 *
 * After init, this is the sole source of engine CLI flags (no hardcoded fallback).
 */
export function getConfiguredArgs(engine: "claude" | "codex", mode: "interactive" | "print" | "stream"): string[] {
  const engineConfig = loadConfig()[engine];
  if (!engineConfig) return [];

  const globalArgs = engineConfig.args ?? [];
  const modeArgs = engineConfig[mode] ?? [];

  if (mode === "stream") {
    const printArgs = engineConfig.print ?? [];
    return [...globalArgs, ...printArgs, ...modeArgs];
  }

  return [...globalArgs, ...modeArgs];
}

export function getDefaultSceneId(): string {
  return loadConfig().scenes?.default ?? "default";
}

export function getUserSceneAliases(): Record<string, string> {
  return loadConfig().scenes?.aliases ?? {};
}

export function loadUserScenes(): Map<string, string> {
  if (_userScenes !== undefined) return _userScenes;
  _userScenes = new Map();

  const scenesDir = join(getConfigDir(), "scenes");

  if (!existsSync(scenesDir)) return _userScenes;

  try {
    for (const entry of readdirSync(scenesDir, { withFileTypes: true })) {
      if (!entry.isFile() || !entry.name.endsWith(".md")) continue;
      const name = basename(entry.name, ".md");
      _userScenes.set(name, readFileSync(join(scenesDir, entry.name), "utf-8"));
    }
  } catch {
    // ignore read errors silently
  }

  return _userScenes;
}
