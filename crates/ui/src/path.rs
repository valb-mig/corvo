// A path cut at the front to fit `most` characters, whole folders at a time, so its end, which
// tells it apart, stays: `…/src/home`.
pub fn tail(path: &str, most: usize) -> String {
    if path.chars().count() <= most {
        return path.to_owned();
    }
    let parts: Vec<&str> = path.split('/').collect();
    for start in 1..parts.len() {
        let rest = parts[start..].join("/");
        if rest.chars().count() + 2 <= most {
            return format!("…/{rest}");
        }
    }
    format!("…/{}", parts.last().copied().unwrap_or_default())
}
