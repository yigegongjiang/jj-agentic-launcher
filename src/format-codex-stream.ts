// Sources of truth for the fields rendered in this formatter, verified on 2026-03-14:
// 1) https://raw.githubusercontent.com/openai/codex/main/codex-rs/exec/src/exec_events.rs
// 2) https://raw.githubusercontent.com/openai/codex/main/codex-rs/protocol/src/models.rs
// 3) Secondary cross-check only: https://raw.githubusercontent.com/openai/codex/main/sdk/typescript/src/items.ts
// 4) Secondary cross-check only: https://raw.githubusercontent.com/openai/codex/main/sdk/typescript/src/events.ts
//
// Important boundary:
// - `jj-prompt-launcher` keeps using the local `codex exec --json`.
// - OpenAI's TS SDK does not currently expose a parse-only API for an already-running external
//   `codex --json` stream, so this file intentionally parses protocol-shaped JSON objects itself.
// - When the Rust protocol and TS SDK types diverge, treat the Rust protocol sources above as primary.

import { truncate, previewValue } from "./format-utils";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function getString(record: Record<string, unknown>, key: string): string | undefined {
  const value = record[key];
  return typeof value === "string" ? value : undefined;
}

function getNumber(record: Record<string, unknown>, key: string): number | undefined {
  const value = record[key];
  return typeof value === "number" ? value : undefined;
}

function getArray(record: Record<string, unknown>, key: string): unknown[] | undefined {
  const value = record[key];
  return Array.isArray(value) ? value : undefined;
}

function getRecord(record: Record<string, unknown>, key: string): Record<string, unknown> | undefined {
  const value = record[key];
  return isRecord(value) ? value : undefined;
}

type TextDeltaOptions = {
  emitPrefixForEmptyFirstChunk?: boolean;
  prefix: string;
  snapshotLabel: string;
};

export class CodexStreamFormatter {
  private readonly commandOutputState = new Map<string, string>();
  private readonly itemSnapshotState = new Map<string, string>();
  private readonly textState = new Map<string, string>();

  format(event: unknown): string {
    if (!isRecord(event)) return this.formatUnknown("unknown_event", event);

    switch (getString(event, "type")) {
      case "thread.started":
        return this.formatThreadStarted(event);
      case "turn.started":
        return "";
      case "turn.completed":
        return this.formatTurnCompleted(event);
      case "turn.failed":
        return this.formatTurnFailed(event);
      case "error":
        return `\n[error] ${getString(event, "message") ?? previewValue(event, 400)}\n`;
      case "item.started":
        return this.formatItemEvent("started", event);
      case "item.updated":
        return this.formatItemEvent("updated", event);
      case "item.completed":
        return this.formatItemEvent("completed", event);
      default:
        return this.formatUnknown(`unknown_event:${getString(event, "type") ?? "unknown"}`, event);
    }
  }

  private formatThreadStarted(event: Record<string, unknown>): string {
    const threadId = getString(event, "thread_id");
    return threadId ? `\n[thread] ${threadId}\n` : "";
  }

  private formatTurnCompleted(event: Record<string, unknown>): string {
    const usage = getRecord(event, "usage");
    if (!usage) return "\n\n---\nturn: completed\n---\n";

    const inputTokens = getNumber(usage, "input_tokens") ?? 0;
    const cachedInputTokens = getNumber(usage, "cached_input_tokens") ?? 0;
    const outputTokens = getNumber(usage, "output_tokens") ?? 0;
    return `\n\n---\ninput_tokens: ${inputTokens}\ncached_input_tokens: ${cachedInputTokens}\noutput_tokens: ${outputTokens}\n---\n`;
  }

  private formatTurnFailed(event: Record<string, unknown>): string {
    const error = getRecord(event, "error");
    const message = error ? getString(error, "message") : undefined;
    return `\n[turn_failed] ${message ?? previewValue(event, 400)}\n`;
  }

  private formatItemEvent(eventType: "started" | "updated" | "completed", event: Record<string, unknown>): string {
    const item = getRecord(event, "item");
    if (!item) return this.formatUnknown(`unknown_item_event:${eventType}`, event);

    switch (getString(item, "type")) {
      case "agent_message":
        return this.formatAgentMessage(item);
      case "reasoning":
        return this.formatReasoning(item);
      case "command_execution":
        return this.formatCommand(eventType, item);
      case "file_change":
        return this.formatFileChange(item);
      case "mcp_tool_call":
        return this.formatMcpCall(eventType, item);
      case "collab_tool_call":
        return this.formatCollabToolCall(eventType, item);
      case "web_search":
        return this.formatWebSearch(item);
      case "todo_list":
        return this.formatTodoList(item);
      case "error":
        return `\n[error] ${getString(item, "message") ?? previewValue(item, 400)}\n`;
      default:
        return this.formatUnknown(`unknown_item:${getString(item, "type") ?? "unknown"}`, item);
    }
  }

  private formatAgentMessage(item: Record<string, unknown>): string {
    const id = getString(item, "id");
    const text = getString(item, "text") ?? "";
    if (!id) return text.length === 0 ? "" : `\n${text}`;
    return this.formatTextDelta(id, text, {
      prefix: "\n",
      snapshotLabel: "message",
    });
  }

  private formatReasoning(item: Record<string, unknown>): string {
    const id = getString(item, "id");
    const text = getString(item, "text") ?? "";
    if (!id) return text.length === 0 ? "\n[thinking] " : `\n[thinking] ${text}`;
    return this.formatTextDelta(id, text, {
      emitPrefixForEmptyFirstChunk: true,
      prefix: "\n[thinking] ",
      snapshotLabel: "thinking",
    });
  }

  private formatCommand(eventType: "started" | "updated" | "completed", item: Record<string, unknown>): string {
    const id = getString(item, "id");
    const output = getString(item, "aggregated_output") ?? "";
    const status = getString(item, "status") ?? "unknown";

    let result = "";
    if (eventType === "started")
      result += `\n[command:${status}] ${getString(item, "command") ?? "(unknown command)"}\n`;

    if (id) {
      const delta = this.formatCommandOutputDelta(id, output);
      if (delta) result += delta;
    } else if (output) {
      result += output;
    }

    if (eventType === "completed") {
      const exitCode = getNumber(item, "exit_code");
      result += `\n[command_exit] ${exitCode === undefined ? status : `${exitCode} (${status})`}\n`;
    }

    return result;
  }

  private formatFileChange(item: Record<string, unknown>): string {
    const status = getString(item, "status") ?? "unknown";
    const changes = (getArray(item, "changes") ?? [])
      .map((change) => {
        if (!isRecord(change)) return previewValue(change, 120);
        return `${getString(change, "kind") ?? "unknown"} ${getString(change, "path") ?? "(unknown path)"}`;
      })
      .join(", ");

    return this.formatSnapshot(
      getString(item, "id"),
      `\n[file_change:${status}] ${changes || "(empty)"}\n`,
      item,
      "file_change",
    );
  }

  private formatMcpCall(eventType: "started" | "updated" | "completed", item: Record<string, unknown>): string {
    const server = getString(item, "server") ?? "(unknown server)";
    const tool = getString(item, "tool") ?? "(unknown tool)";
    const status = getString(item, "status") ?? "unknown";

    if (eventType === "updated") {
      return this.formatSnapshot(getString(item, "id"), "", item, "mcp_tool_call");
    }

    if (eventType === "started") {
      return `\n[mcp:${server}/${tool}:${status}] ${previewValue(item.arguments, 200)}\n`;
    }

    // completed
    const error = getRecord(item, "error");
    if (error) return `\n[mcp_error] ${getString(error, "message") ?? previewValue(error, 300)}\n`;

    const result = getRecord(item, "result");
    if (!result) return `\n[mcp:${server}/${tool}] status=${status}\n`;

    let output = "";
    if (result.structured_content !== undefined)
      output += `\n[mcp_result] ${previewValue(result.structured_content, 500)}\n`;
    if (result.content !== undefined) output += `\n[mcp_content] ${previewValue(result.content, 500)}\n`;
    return output || `\n[mcp_result] ${previewValue(result, 500)}\n`;
  }

  private formatCollabToolCall(eventType: "started" | "updated" | "completed", item: Record<string, unknown>): string {
    const tool = getString(item, "tool") ?? "unknown";
    const status = getString(item, "status") ?? "unknown";
    const sender = getString(item, "sender_thread_id");
    const receivers = getArray(item, "receiver_thread_ids")
      ?.filter((v): v is string => typeof v === "string")
      .join(", ");

    let result = `\n[collab:${tool}:${status}]`;
    if (sender) result += ` sender=${sender}`;
    if (receivers) result += ` receivers=${receivers}`;
    result += "\n";

    const prompt = getString(item, "prompt");
    if (prompt) result += `prompt: ${truncate(prompt, 240)}\n`;

    const agentsStates = getRecord(item, "agents_states");
    if (agentsStates) {
      const entries = Object.entries(agentsStates)
        .map(([agentId, state]) => {
          if (!isRecord(state)) return `${agentId}:${previewValue(state, 80)}`;
          const s = getString(state, "status") ?? "unknown";
          const msg = getString(state, "message");
          return msg ? `${agentId}:${s} (${truncate(msg, 80)})` : `${agentId}:${s}`;
        })
        .join(", ");
      if (entries.length > 0) result += `agents: ${entries}\n`;
    }

    return this.formatSnapshot(getString(item, "id"), result, item, `collab:${tool}:${eventType}`);
  }

  private formatWebSearch(item: Record<string, unknown>): string {
    const action = getRecord(item, "action");
    const actionType = action ? getString(action, "type") : undefined;
    let result = `\n[web_search${actionType ? `:${actionType}` : ""}] ${getString(item, "query") ?? "(unknown query)"}`;

    if (action) {
      if (actionType === "search") {
        const query = getString(action, "query");
        const queries = getArray(action, "queries")
          ?.filter((v): v is string => typeof v === "string")
          .join(" | ");
        if (query && query !== getString(item, "query")) result += `\nquery: ${query}`;
        if (queries && queries.length > 0) result += `\nqueries: ${queries}`;
      } else if (actionType === "open_page" || actionType === "find_in_page") {
        const url = getString(action, "url");
        if (url) result += `\nurl: ${url}`;
        if (actionType === "find_in_page") {
          const pattern = getString(action, "pattern");
          if (pattern) result += `\npattern: ${pattern}`;
        }
      } else {
        result += `\naction: ${previewValue(action, 300)}`;
      }
    }

    return this.formatSnapshot(getString(item, "id"), `${result}\n`, item, "web_search");
  }

  private formatTodoList(item: Record<string, unknown>): string {
    const todos = getArray(item, "items") ?? [];
    const content = todos
      .map((todo) => {
        if (!isRecord(todo)) return `[?] ${previewValue(todo, 120)}`;
        return `${todo.completed === true ? "[x]" : "[ ]"} ${getString(todo, "text") ?? "(empty)"}`;
      })
      .join("\n");

    return this.formatSnapshot(
      getString(item, "id"),
      content.length === 0 ? "" : `\n[todo]\n${content}\n`,
      item,
      "todo_list",
    );
  }

  private formatTextDelta(id: string, nextValue: string, options: TextDeltaOptions): string {
    const previousValue = this.textState.get(id);
    this.textState.set(id, nextValue);

    if (previousValue === undefined) {
      if (nextValue.length === 0) return options.emitPrefixForEmptyFirstChunk ? options.prefix : "";
      return `${options.prefix}${nextValue}`;
    }

    if (nextValue === previousValue) return "";
    if (nextValue.startsWith(previousValue)) return nextValue.slice(previousValue.length);
    if (nextValue.length === 0) return "";
    return `\n[${options.snapshotLabel}_snapshot] ${nextValue}`;
  }

  private formatSnapshot(id: string | undefined, rendered: string, rawItem: unknown, fallbackLabel: string): string {
    if (!id) return rendered.length === 0 ? this.formatUnknown(fallbackLabel, rawItem) : rendered;

    const snapshot = safeJson(rawItem);
    const previousSnapshot = this.itemSnapshotState.get(id);
    this.itemSnapshotState.set(id, snapshot);
    return snapshot === previousSnapshot ? "" : rendered;
  }

  private formatCommandOutputDelta(id: string, nextValue: string): string {
    const previousValue = this.commandOutputState.get(id);
    this.commandOutputState.set(id, nextValue);

    if (previousValue === undefined) return nextValue;
    if (nextValue === previousValue) return "";
    if (nextValue.startsWith(previousValue)) return nextValue.slice(previousValue.length);
    return `\n[command_output_snapshot]\n${nextValue}`;
  }

  private formatUnknown(label: string, value: unknown): string {
    return `\n[${label}] ${previewValue(value, 800)}\n`;
  }
}
