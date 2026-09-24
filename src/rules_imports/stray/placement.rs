//! Scope tracking for stray `use` detection.
//!
//! Brace scopes separate file tops, test modules, macro spans, and plain
//! blocks; headers seal on the first other item while macro spans stay
//! opaque template text.

use crate::{
    lexer::marker::cut_line_comment,
    rules_imports::{
        push_hit,
        rank::is_use_start,
        stray::classify::{
            is_filler, is_macro_def_open, is_quote_open, mod_name_before, seals_header,
            undecided_mod,
        },
    },
    scan::SourceFile,
};

/// Inline scope kinds tracked while scanning brackets.
enum Scope {
    /// Macro brace span holding template text.
    MacroBrace,
    /// Macro bracket span holding template text.
    MacroBracket,
    /// Macro paren span holding template text.
    MacroParen,
    /// Inline `mod` body with test flag and sealed flag.
    Mod {
        /// True for `mod tests` bodies with their own header.
        test: bool,
        /// True once code sealed this module header.
        sealed: bool,
    },
    /// Any other brace block such as function or impl bodies.
    Other,
}

/// Scan state for stray `use` checks in one file.
#[derive(Default)]
struct StrayState {
    /// True once file-top code sealed the import header.
    file_sealed: bool,
    /// Undecided `mod name` awaiting its opener or terminator.
    pending_mod: Option<String>,
    /// Open brace scopes from outermost to innermost.
    scopes: Vec<Scope>,
}

impl StrayState {
    /// True if any enclosing scope holds template text.
    fn in_macro(&self) -> bool {
        self.scopes.iter().any(|scope| {
            matches!(
                scope,
                Scope::MacroBrace | Scope::MacroBracket | Scope::MacroParen
            )
        })
    }

    /// True if a `use` opening at the current scope flags as stray.
    ///
    /// Template spans silence everything beneath them; plain blocks flag
    /// everywhere while module headers flag once sealed.
    fn is_stray(&self) -> bool {
        if self.in_macro() {
            return false;
        }
        let in_block = self
            .scopes
            .iter()
            .any(|scope| matches!(scope, Scope::Other));
        let boundary = self.scopes.iter().rev().find_map(|scope| match scope {
            Scope::Mod { test: true, sealed } => Some(*sealed || in_block),
            Scope::Mod { .. } => Some(in_block),
            _ => None,
        });
        boundary.unwrap_or(self.file_sealed || in_block)
    }

    /// Seal the current header scope when `code` holds a sealing item.
    fn seal_current(&mut self, code: &str) {
        if !seals_header(code) {
            return;
        }
        match self.scopes.last_mut() {
            Some(Scope::Mod { test: true, sealed }) => {
                *sealed = true;
            }
            Some(_) => {}
            None => {
                self.file_sealed = true;
            }
        }
    }

    /// Push an inline `mod` scope for `name`.
    fn push_mod(&mut self, name: &str) {
        self.scopes.push(Scope::Mod {
            test: name == "tests",
            sealed: false,
        });
    }

    /// Track one `{` opener; returns true for modules and macro spans.
    fn open_curly(&mut self, code: &str, pos: usize) -> bool {
        if let Some(name) = mod_name_before(code, pos) {
            self.pending_mod = None;
            self.push_mod(&name);
            return true;
        }
        if is_macro_def_open(code, pos) || is_quote_open(code, pos) {
            self.pending_mod = None;
            self.scopes.push(Scope::MacroBrace);
            return true;
        }
        if self.pending_is_bare(code, pos) {
            let name = self.pending_mod.take().unwrap_or_default();
            self.push_mod(&name);
            return true;
        }
        self.scopes.push(Scope::Other);
        false
    }

    /// Track one open bracket at byte `pos` in `code`; returns true for modules.
    ///
    /// Braces open modules, macro definitions, quote-family spans, or plain
    /// blocks; parens and brackets only open quote-family spans, so ordinary
    /// calls and index expressions never touch the scope stack.
    fn open_bracket(&mut self, code: &str, pos: usize, c: char) -> bool {
        if c == '{' {
            return self.open_curly(code, pos);
        }
        if is_quote_open(code, pos) {
            self.scopes.push(macro_span(c));
            return true;
        }
        false
    }

    /// True if an undecided `mod` awaits a bare opener at byte `pos`.
    ///
    /// Only an otherwise empty prefix counts, so later items with their own
    /// opener never inherit a stale declaration.
    fn pending_is_bare(&mut self, code: &str, pos: usize) -> bool {
        if self.pending_mod.is_none() {
            return false;
        }
        if code
            .get(..pos)
            .is_some_and(|prefix| prefix.trim().is_empty())
        {
            return true;
        }
        self.pending_mod = None;
        false
    }

    /// Close scopes for bracket `c`.
    fn close_bracket(&mut self, c: char) {
        let closer = match self.scopes.last() {
            Some(Scope::MacroBrace) => Some('}'),
            Some(Scope::MacroBracket) => Some(']'),
            Some(Scope::MacroParen) => Some(')'),
            Some(Scope::Mod { .. } | Scope::Other) | None => None,
        };
        if closer == Some(c) {
            self.scopes.truncate(self.scopes.len().saturating_sub(1));
            return;
        }
        if closer.is_none() && c == '}' {
            self.scopes.truncate(self.scopes.len().saturating_sub(1));
        }
    }

    /// Visit one bracket character; returns true when a module opened.
    fn visit_brace(&mut self, code: &str, pos: usize, c: char) -> bool {
        if !matches!(c, '{' | '(' | '[') {
            self.close_bracket(c);
            return false;
        }
        self.open_bracket(code, pos, c)
    }

    /// Scan the brackets of one non-`use` line and track scopes.
    fn visit_braces(&mut self, code: &str) {
        let mut pushed_mod = false;
        for (pos, c) in code.char_indices() {
            pushed_mod |= self.visit_brace(code, pos, c);
        }
        self.track_pending(code, pushed_mod);
    }

    /// Clear an undecided `mod` on deciding lines.
    fn clear_pending(&mut self, code: &str) {
        if !is_filler(code) && !code.contains('{') {
            self.pending_mod = None;
        }
    }

    /// Update the undecided `mod` across filler and deciding lines.
    fn track_pending(&mut self, code: &str, pushed_mod: bool) {
        if pushed_mod {
            return;
        }
        if self.pending_mod.is_some() {
            self.clear_pending(code);
            return;
        }
        if let Some(name) = undecided_mod(code) {
            self.pending_mod = Some(name);
        }
    }

    /// Visit one non-`use` line for sealing and bracket tracking.
    fn visit_code(&mut self, code: &str) {
        self.seal_current(code);
        self.visit_braces(code);
    }
}

/// Macro span variant opening with bracket `c`.
/// Macro span variant opening with bracket `c`.
const fn macro_span(c: char) -> Scope {
    match c {
        '(' => Scope::MacroParen,
        '[' => Scope::MacroBracket,
        _ => Scope::MacroBrace,
    }
}

/// Stray `use` findings for one file.
#[must_use]
pub fn check_file(file: &SourceFile) -> Vec<String> {
    let mut state = StrayState::default();
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < file.lines.len() {
        idx = stray_step(file, idx, &mut state, &mut out);
    }
    out
}

/// Advance the stray scan by one statement; returns the next index.
fn stray_step(
    file: &SourceFile,
    idx: usize,
    state: &mut StrayState,
    out: &mut Vec<String>,
) -> usize {
    let Some(stripped) = file.stripped.get(idx) else {
        return idx.saturating_add(1);
    };
    if file.lines.get(idx).is_none() {
        return idx.saturating_add(1);
    }
    let code = cut_line_comment(stripped);
    if is_use_start(code) {
        let end = use_end(&file.stripped, idx);
        if state.is_stray() {
            push_hit(out, &file.path, &file.lines, idx);
        }
        return end.saturating_add(1);
    }
    state.visit_code(code);
    idx.saturating_add(1)
}

/// End line of the `use` statement opening at `start`.
fn use_end(stripped: &[String], start: usize) -> usize {
    let mut end = start;
    while end < stripped.len()
        && !cut_line_comment(stripped.get(end).map_or("", |line| line)).contains(';')
    {
        end = end.saturating_add(1);
    }
    end
}
