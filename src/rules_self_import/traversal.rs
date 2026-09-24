//! Per-file scanner for self-resolving imports.

use crate::{
    lexer::{literal::strip_strings, marker::cut_line_comment},
    rules_self_import::{
        expand::{attr_resume, expand},
        resolve::{char_line_starts, is_self_hit, resolve_full, strip_alias},
        token::{match_cfg_test, match_mod_decl, match_use_kw},
    },
};

/// Per-file self-import scanner holding state-machine state.
#[derive(Debug)]
pub struct SelfScan<'a> {
    /// Scanned characters of the joined file text.
    chars: Vec<char>,
    /// Character-offset line starts.
    starts: Vec<usize>,
    /// Original file lines used in findings.
    lines: &'a [String],
    /// File path used in findings.
    path: &'a str,
    /// Module path of the current file.
    cur: &'a [String],
    /// Current brace nesting depth.
    depth: usize,
    /// Enclosing inline modules with test flags.
    stack: Vec<(String, usize, bool)>,
    /// True while a cfg-test marker awaits its module.
    pending_test: bool,
    /// Module declaration awaiting its opening brace.
    pending_mod: Option<(String, bool)>,
    /// True while a use statement may legally follow.
    expect: bool,
    /// Collected self-import findings.
    out: Vec<String>,
}

impl<'a> SelfScan<'a> {
    /// Check helper exposed to the parent index.
    #[must_use]
    pub fn scan_file(path: &'a str, lines: &'a [String], cur: &'a [String]) -> Vec<String> {
        Self::check(path, lines, cur)
    }

    /// Scan one file for self-resolving imports.
    fn check(path: &'a str, lines: &'a [String], cur: &'a [String]) -> Vec<String> {
        let code_lines: Vec<String> = lines
            .iter()
            .map(|l| cut_line_comment(&strip_strings(l)).to_owned())
            .collect();
        let text: String = code_lines.join("\n");
        let chars: Vec<char> = text.chars().collect();
        let starts = char_line_starts(&chars);
        Self {
            chars,
            starts,
            lines,
            path,
            cur,
            depth: 0,
            stack: Vec::new(),
            pending_test: false,
            pending_mod: None,
            expect: true,
            out: Vec::new(),
        }
        .run()
    }

    /// Run the scan to completion.
    fn run(mut self) -> Vec<String> {
        let mut i = 0;
        while i < self.chars.len() {
            i = self.step(i);
        }
        self.out
    }

    /// Advance one token at `i`; returns the resume index.
    fn step(&mut self, i: usize) -> usize {
        let Some(ch) = self.chars.get(i).copied() else {
            return i.saturating_add(1);
        };
        if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
            return i.saturating_add(1);
        }
        if ch == '#' {
            return self.step_hash(i);
        }
        if ch.is_alphabetic() || ch == '_' {
            return self.step_word(i);
        }
        self.step_punct(i)
    }

    /// Advance past attributes, tracking cfg-test markers.
    fn step_hash(&mut self, i: usize) -> usize {
        let n = self.chars.len();
        let mut j = i.saturating_add(1);
        while self.chars.get(j).is_some_and(|c| *c == ' ' || *c == '\t') {
            j = j.saturating_add(1);
        }
        if j >= n || self.chars.get(j).is_none_or(|c| *c != '[') {
            return i.saturating_add(1);
        }
        if let Some(end) = match_cfg_test(&self.chars, i) {
            self.pending_test = true;
            return end;
        }
        self.skip_attr(i)
    }

    /// Advance past a bracketed attribute from `i`.
    fn skip_attr(&mut self, i: usize) -> usize {
        let next = attr_resume(&self.chars, i);
        self.expect = true;
        next
    }

    /// Advance past an identifier word at `i`.
    fn step_word(&mut self, i: usize) -> usize {
        if let Some((name, kind, end, kind_at)) = match_mod_decl(&self.chars, i)
            && (i == 0
                || self
                    .chars
                    .get(i.saturating_sub(1))
                    .is_some_and(|c| !c.is_alphanumeric() && *c != '_'))
        {
            return self.step_mod(&name, kind, end, kind_at);
        }
        self.maybe_use(i)
    }

    /// Record a module declaration at `i`; returns the resume index.
    fn step_mod(&mut self, name: &str, kind: char, end: usize, kind_at: usize) -> usize {
        if kind == ';' {
            self.pending_test = false;
            self.expect = true;
            return end;
        }
        self.pending_mod = Some((name.to_owned(), self.pending_test || name == "tests"));
        self.pending_test = false;
        kind_at
    }

    /// Advance past a use statement at `i`, if any; else skip the word.
    fn maybe_use(&mut self, i: usize) -> usize {
        if self.expect
            && let Some(use_end) = match_use_kw(&self.chars, i)
        {
            return self.step_use(i, use_end);
        }
        self.skip_ident(i)
    }

    /// Advance past a use statement; records self-resolving imports.
    fn step_use(&mut self, i: usize, use_end: usize) -> usize {
        if i > 0
            && (self
                .chars
                .get(i.saturating_sub(1))
                .is_some_and(|c| c.is_alphanumeric())
                || self
                    .chars
                    .get(i.saturating_sub(1))
                    .is_some_and(|c| *c == '_')
                || self
                    .chars
                    .get(i.saturating_sub(1))
                    .is_some_and(|c| *c == '#'))
        {
            return self.skip_ident(i);
        }
        let n = self.chars.len();
        let j = stmt_end(&self.chars, use_end);
        let sl = self.starts.partition_point(|&s| s <= i);
        let skip = self.pending_test;
        self.pending_test = false;
        let next = if j < n { j.saturating_add(1) } else { n };
        self.expect = true;
        if skip {
            return next;
        }
        let body = self
            .chars
            .get(use_end..j)
            .map_or_else(String::new, |part| part.iter().collect());
        if let Some(hit) = self.use_finding(body.trim(), sl) {
            self.out.push(hit);
        }
        next
    }

    /// Finding for one use body resolving into the current module, if any.
    fn use_finding(&self, body: &str, sl: usize) -> Option<String> {
        if self.stack.iter().any(|(_, _, test)| *test) {
            return None;
        }
        let curpath = self.curpath();
        expand(body)
            .into_iter()
            .find_map(|ep| self.check_expansion(&curpath, sl, &ep))
    }

    /// Finding for one expanded path, if it resolves into this module.
    fn check_expansion(&self, curpath: &[String], sl: usize, ep: &str) -> Option<String> {
        let ep = strip_alias(ep);
        if ep.is_empty() {
            return None;
        }
        let segs: Vec<String> = ep
            .split("::")
            .filter(|s| !s.is_empty())
            .map(|s| s.strip_prefix("r#").unwrap_or(s).to_owned())
            .collect();
        if segs.is_empty() {
            return None;
        }
        let full = resolve_full(&segs, curpath)?;
        is_self_hit(&full, curpath)
            .then(|| self.finding_at(sl))
            .flatten()
    }

    /// Format a finding from the original line starting a use statement.
    fn finding_at(&self, sl: usize) -> Option<String> {
        self.lines
            .get(sl.saturating_sub(1))
            .map(|orig| format!("{}:{sl}:{orig}", self.path))
    }

    /// Current module path including enclosing inline modules.
    fn curpath(&self) -> Vec<String> {
        self.cur
            .iter()
            .chain(
                self.stack
                    .iter()
                    .filter(|(name, _, _)| name != "<test>")
                    .map(|(name, _, _)| name),
            )
            .cloned()
            .collect()
    }

    /// Advance past punctuation, tracking modules and statement ends.
    fn step_punct(&mut self, i: usize) -> usize {
        match self.chars.get(i).copied() {
            Some('{') => self.open_brace(),
            Some('}') => self.close_brace(),
            Some(';') => {
                self.pending_test = false;
                self.expect = true;
            }
            _ => self.expect = false,
        }
        i.saturating_add(1)
    }

    /// Push a pending module or test scope onto the stack.
    fn open_brace(&mut self) {
        self.depth = self.depth.saturating_add(1);
        match self.pending_mod.take() {
            Some((name, is_test)) => self.push_scope(name, is_test),
            None => self.push_test_scope(),
        }
        self.expect = true;
    }

    /// Push a named module scope onto the stack.
    fn push_scope(&mut self, name: String, is_test: bool) {
        self.stack.push((name, self.depth, is_test));
    }

    /// Push a test scope for a bare cfg-test marker.
    fn push_test_scope(&mut self) {
        if self.pending_test {
            self.stack.push(("<test>".to_owned(), self.depth, true));
            self.pending_test = false;
        }
    }

    /// Pop modules closed by a brace.
    fn close_brace(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        while self
            .stack
            .last()
            .is_some_and(|(_, level, _)| *level > self.depth)
        {
            let kept = self.stack.len().saturating_sub(1);
            self.stack.truncate(kept);
        }
        self.expect = true;
    }

    /// Advance past a plain identifier word.
    fn skip_ident(&mut self, i: usize) -> usize {
        let mut j = i;
        while self
            .chars
            .get(j)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            j = j.saturating_add(1);
        }
        self.expect = false;
        j
    }
}

/// End of the use statement opened at `from`.
fn stmt_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    let mut depth: i32 = 0;
    while let Some(current) = chars.get(j).copied() {
        match current {
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => break,
            _ => {}
        }
        j = j.saturating_add(1);
    }
    j
}
