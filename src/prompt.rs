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

pub fn user_choice<T: std::fmt::Display>(choices: &[T]) -> std::io::Result<&T> {
    let mut stdout = std::io::stdout();
    let mut i = 0;
    write!(stdout, "Select a container:\n")?;
    for choice in choices {
        i += 1;
        write!(stdout, "  {}. {}\n", i, choice)?;
    }
    loop {
        write!(stdout, "> ")?;
        stdout.flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        let index = match line.trim().parse::<usize>() {
            Ok(i) => i,
            Err(_) => continue,
        };
        match choices.get(index - 1) {
            Some(ref c) => break Ok(c),
            None => continue,
        }
    }
}
