//! M0 spike: terminal-in-GPUI (SPEC §18).

fn greeting() -> &'static str {
    "hello"
}

fn main() {
    println!("{}", greeting());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets() {
        assert_eq!(greeting(), "hello");
    }
}
