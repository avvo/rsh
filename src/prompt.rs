use std;
use std::io::Write;

pub fn prompt_with_default(prompt: &str, default: Option<String>) -> std::io::Result<String> {
    let mut stdout = std::io::stdout();
    let mut result = String::new();
    let prompt = match default {
        Some(ref v) => format!("{} ({}): ", prompt, v),
        None => format!("{}: ", prompt),
    };
    write!(stdout, "{}", prompt)?;
    stdout.flush()?;
    std::io::stdin().read_line(&mut result)?;
    if result.chars().last() == Some('\n') {
        result.pop();
    }
    if result.chars().last() == Some('\r') {
        result.pop();
    }
    match (result.as_ref(), default) {
        ("", Some(v)) => Ok(v),
        _ => Ok(result),
    }
}
