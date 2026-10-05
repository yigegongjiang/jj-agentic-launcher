use crate::config::get_config_dir;
use crate::parse::DEFAULT_MAX_ITER;
use crate::scenes::{default_scene, list_all_scene_names, Engine};

pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
// GitHub owner/repo slug used to build release URLs. Kept as a const rather than
// `CARGO_PKG_REPOSITORY` (a full URL) because the update/install contract needs
// the slug form.
pub const REPO: &str = "yigegongjiang/jj-agentic-launcher";

pub fn build_help_text() -> String {
    let scenes = list_all_scene_names().join(", ");
    let config_dir = get_config_dir();
    let config_dir = config_dir.display();
    let dflt = default_scene();
    let dflt_engine = match dflt.engine {
        Engine::Claude => "Claude Code",
        Engine::Codex => "Codex",
        Engine::Agy => "agy",
    };
    let dflt = format!("{} ({dflt_engine})", dflt.scene_id);

    format!(
        "{NAME} {VERSION} — Launch Claude Code / Codex / agy with shared scene prompts

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

Pre-command:
  --pre '<cmd>'                           Run <cmd> in an interactive $SHELL, then exec the engine in
                                          that same shell, inheriting whatever the command left behind
                                          (cwd, exported vars, sourced state, shell functions):
                                            {NAME} --pre 'j api' it 'explain the architecture'
                                            {NAME} --pre 'cd $(fd -t d | fzf)' d 'review this'
                                            {NAME} --pre 'source .venv/bin/activate' code 'run tests'
                                          Runs before every child spawn — each --loop iteration and
                                          each `<<>>` step (shell state cannot outlive its process).
                                          A non-zero exit aborts before the engine starts.
                                          POSIX sh syntax; $SHELL must be sh/bash/zsh (not fish).

Prompt is a single positional argument. Use shell quoting for any complexity:
  {NAME} d 'multi-line
prompt with $vars, \"quotes\", \\ backslashes — POSIX single-quote keeps it literal'

Sequential prompts:
  Embed `<<>>` to split one prompt into N independent single-shots, run in order:
    {NAME} d 'step 1 <<>> step 2 <<>> step 3'
  Cannot combine with --loop (each segment runs exactly once).

Passthrough to claude/codex/agy:
  Everything after `--` is forwarded verbatim to the underlying engine,
  appended after scene injection and before the prompt (no validation):
    {NAME} code 'fix the bug' -- --model opus --add-dir ../shared
    {NAME} .d -- --search          (Codex flag, REPL with no prompt)
    {NAME} ,d 'hi' -- --model gemini-3.7-flash-high     (agy flag)

Scenes:
  {scenes}
  Claude Code by default; . prefix for Codex, , prefix for agy (Antigravity):
    {NAME} d / .d / ,d
  agy has no system-prompt flag, so its scene is sent as prompt text
  (wrapped in <system_instructions>); a REPL gets it as the priming turn.

Default scene:
  {dflt}
  Set by `scenes.default` in config.json — an alias (it), a scene file name
  (it-expert), or an engine prefix to change the default engine (.it / ,it).
  Editable any time.
  The scene argument is optional everywhere; each of these uses the default:
    {NAME}                omitted entirely (REPL)
    {NAME} 'prompt'       lone argument that is not a known scene
    {NAME} -p 'prompt'    -p / --loop already imply a prompt
    {NAME} '' 'prompt'    empty token
    {NAME} . 'prompt'     bare . -> default scene, forced onto Codex ( , -> agy)
  Two arguments -> the first MUST be a scene (unknown name = error, not a prompt).

Meta commands:
  help, --help, -h            Show this help message
  version, --version, -v      Show version information
  update, upgrade             Download the latest release and replace this binary
  uninstall                   Remove this binary from disk
  ext                         Switch Claude / Codex MCP, skills, plugins: globally or per project
                              (`{NAME} ext help`)

Config:
  {config_dir}/config.json    Launch arguments
  {config_dir}/scenes/*.md    Custom scenes
"
    )
}
