//! Quote tracking for TOML scans.
//!
//! The tracker feeds one byte at a time so comment marks, braces, and key
//! tokens inside strings never scan as syntax.

/// Quote tracking for one TOML scan.
#[derive(Clone, Copy)]
struct Quotes {
    /// Inside a basic string.
    double: bool,
    /// Inside a literal string.
    single: bool,
    /// Previous char escapes this one.
    escaped: bool,
}

/// New tracker outside any string.
const fn fresh_quotes() -> Quotes {
    Quotes {
        double: false,
        single: false,
        escaped: false,
    }
}

/// Next tracker plus true if `byte` is live code outside string bodies.
///
/// Quote marks count as live so quoted keys still match while string
/// bodies stay hidden.
const fn live_code(quotes: Quotes, byte: u8) -> (Quotes, bool) {
    if quotes.escaped {
        let next = Quotes {
            escaped: false,
            ..quotes
        };
        return (next, false);
    }
    if quotes.double && byte == b'\\' {
        let next = Quotes {
            escaped: true,
            ..quotes
        };
        return (next, false);
    }
    if !quotes.single && byte == b'"' {
        let next = Quotes {
            double: !quotes.double,
            ..quotes
        };
        return (next, true);
    }
    if !quotes.double && byte == b'\'' {
        let next = Quotes {
            single: !quotes.single,
            ..quotes
        };
        return (next, true);
    }
    if quotes.double || quotes.single {
        return (quotes, false);
    }
    (quotes, true)
}

/// Indexes with bytes of live code outside string bodies.
#[must_use]
pub fn live_bytes(text: &str) -> Vec<(usize, u8)> {
    let bytes = text.as_bytes();
    let mut quotes = fresh_quotes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes.get(i).copied().unwrap_or(0);
        let (next, plain) = live_code(quotes, byte);
        quotes = next;
        if plain {
            out.push((i, byte));
        }
        i = i.saturating_add(1);
    }
    out
}
