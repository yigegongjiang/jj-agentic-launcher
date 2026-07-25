use crate::config::get_config_dir;
use crate::parse::DEFAULT_MAX_ITER;
use crate::scenes::list_all_scene_names;

pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
// GitHub owner/repo slug used to build release URLs. Kept as a const rather than
// `CARGO_PKG_REPOSITORY` (a full URL) because the update/install contract needs
// the slug form.
pub const REPO: &str = "yigegongjiang/jj-prompt-launcher";

pub fn build_help_text() -> String {
    let scenes = list_all_scene_names().join(", ");
    let config_dir = get_config_dir();
    let config_dir = config_dir.display();

    format!(
        "{NAME} {VERSION} — Launch Claude Code or Codex with shared scene prompts

Usage:
  {NAME} [scene]                         Interactive REPL
  {NAME} [scene] 'prompt'                Single-shot run with stream-JSON renderer (default)
  {NAME} -p [scene] 'prompt'             Single-shot run with raw print passthrough
  {NAME} --loop N [scene] 'prompt'       Run the same single-shot N times serially
  {NAME} --loop relay [scene] 'prompt'   Relay loop: each turn picks up previous turn's handoff
                                                     (status/next_actions). Stops on status=\"end\" or --max-iter
  {NAME} --loop refine [scene] 'prompt'  Refine loop: each turn runs the ORIGINAL prompt verbatim
                                                     in a fresh agent (no cross-turn carry-over except the
                                                     end/continue signal). Stops on status=\"end\" or --max-iter

Loop options:
  --max-iter N                            Safety cap for --loop relay / --loop refine
                                          (default {DEFAULT_MAX_ITER})

Prompt is a single positional argument. Use shell quoting for any complexity:
  {NAME} d 'multi-line
prompt with $vars, \"quotes\", \\ backslashes — POSIX single-quote keeps it literal'

Sequential prompts:
  Embed `<<>>` to split one prompt into N independent single-shots, run in order:
    {NAME} d 'step 1 <<>> step 2 <<>> step 3'
  Cannot combine with --loop (each segment runs exactly once).

Passthrough to claude/codex:
  Everything after `--` is forwarded verbatim to the underlying engine,
  appended after scene injection and before the prompt (no validation):
    {NAME} code 'fix the bug' -- --model opus --add-dir ../shared
    {NAME} .d -- --search          (Codex flag, REPL with no prompt)

Scenes:
  {scenes}
  Default is Claude Code, use . prefix for Codex, e.g. .d / .code

Meta commands:
  help, --help, -h            Show this help message
  version, --version, -v      Show version information
  update, upgrade             Download the latest release and replace this binary
  uninstall                   Remove this binary from disk

Config:
  {config_dir}/config.json    Launch arguments
  {config_dir}/scenes/*.md    Custom scenes
"
    )
}
