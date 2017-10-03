use std::collections::HashMap;

pub fn expand(string: &str, map: HashMap<char, String>) -> Result<String, char> {
    let mut replace_next = false;
    let mut res = Vec::new();
    for c in string.chars() {
        if replace_next && c == '%' {
            replace_next = false;
            res.push(c);
        } else if replace_next {
            replace_next = false;
            match map.get(&c) {
                Some(string) => res.extend(string.chars()),
                None => return Err(c),
            };
        } else if c == '%' {
            replace_next = true;
        } else {
            res.push(c);
        }
    }
    Ok(res.iter().collect())
}
