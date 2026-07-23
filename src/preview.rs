/// Render a launch preview line (`+ cmd args...`) to mirror shell trace output.
/// Control chars are escaped for display; args needing quoting are POSIX-quoted.
pub fn render_launch_preview(cmd: &[String]) -> String {
    let parts: Vec<String> = cmd
        .iter()
        .map(|a| shell_quote(&escape_control_chars(a)))
        .collect();
    format!("+ {}\n", parts.join(" "))
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    // Quote unless every char is in the safe set [a-zA-Z0-9_.=:/@+,-].
    let needs_quote = arg
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || "_.=:/@+,-".contains(c)));
    if !needs_quote {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn escape_control_chars(text: &str) -> String {
    text.replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
