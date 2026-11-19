//! Text scanners of the Livewire module: the PHP component class (properties, public methods, attributes, events,
//! validation rules, the view it renders), the Blade template (component uses, `wire:` bindings, events) and the
//! route files (`Route::get('/x', X::class)`). Everything works on the source text with the shared scanners
//! (`core::srcscan`): comments are blanked, and a second copy with the string contents blanked (`mask`) serves the
//! structure regexes, so a `function` or a brace inside a string never counts. Offsets are the same in all copies.

use std::sync::LazyLock;

use regex::Regex;

use crate::core::LineIndex;
use crate::core::srcscan::{Dialect, matching, split_args, split_top, strip_comments};

/// A regex compiled once.
macro_rules! re {
    ($pattern:literal) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect("valid regex"));
        &*RE
    }};
}

/// A declared property of a component or form class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Prop {
    pub(super) name: String,
    pub(super) public: bool,
    /// `#[Url]`.
    pub(super) url: bool,
    /// The declared type as written (without `?`).
    pub(super) ty: Option<String>,
}

/// A method of a component class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Meth {
    pub(super) name: String,
    pub(super) public: bool,
    /// `#[Computed]`: read as a property, never an action.
    pub(super) computed: bool,
}

/// An event name at a place of the class (a dispatch site, or the method that listens).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub(super) name: String,
    pub(super) line: u32,
    pub(super) method: Option<String>,
}

/// Where a component gets its view from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ViewRef {
    /// `render()` returns `view('livewire.counter')` (dotted name).
    Named(String),
    /// No `render()`: the default `resources/views/livewire/<kebab>.blade.php`.
    Convention,
    /// `render()` exists, but not with a literal view name (inline Blade, a variable, a package view).
    Dynamic,
}

/// One component (or `Livewire\Form`) class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Class {
    pub(super) rel: String,
    /// The short class name.
    pub(super) name: String,
    pub(super) fqn: String,
    /// `extends Livewire\Form`: not a component, but its properties and rules belong to the parent's `$form`.
    pub(super) form: bool,
    /// The component name used in tags: `admin.user-table`.
    pub(super) comp_name: String,
    pub(super) line: u32,
    pub(super) props: Vec<Prop>,
    pub(super) methods: Vec<Meth>,
    /// No traits, no `__call` / `__get`: the members listed are all there is (otherwise no "missing" finding).
    pub(super) complete: bool,
    pub(super) view: ViewRef,
    /// Validation fields with the required flag.
    pub(super) rules: Vec<(String, bool)>,
    pub(super) dispatches: Vec<Site>,
    pub(super) listens: Vec<Site>,
    /// Skipped items: `(reason code, line)`.
    pub(super) dynamic: Vec<(&'static str, u32)>,
}

/// One `wire:` binding of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bind {
    /// `wire:click`, `wire:model`.
    pub(super) attr: String,
    /// The attribute value as written.
    pub(super) value: String,
    pub(super) line: u32,
}

/// What a use of a component names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    /// `<livewire:admin.user-table>` / `@livewire('admin.user-table')`.
    Name(String),
    /// `@livewire(Counter::class)`.
    Class(String),
}

/// One use of a component in a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UseRef {
    pub(super) target: Target,
    pub(super) line: u32,
}

/// What a Blade template contributes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct ViewScan {
    pub(super) uses: Vec<UseRef>,
    pub(super) models: Vec<Bind>,
    pub(super) actions: Vec<Bind>,
    /// Events the template dispatches (`$dispatch('x')`) and listens to on the client (`Livewire.on('x')`).
    pub(super) dispatches: Vec<(String, u32)>,
    pub(super) listens: Vec<(String, u32)>,
    pub(super) dynamic: Vec<(&'static str, u32)>,
}

/// `Route::get('/counter', Counter::class)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RouteUse {
    pub(super) file: String,
    pub(super) line: u32,
    pub(super) verb: String,
    pub(super) uri: String,
    /// The class as written, qualified through the file's `use` lines when possible.
    pub(super) class: String,
    pub(super) name: Option<String>,
}

// ------------------------------------------------------------------------------------------------ text helpers

/// The text with the contents of the string literals blanked (the quotes and the length stay).
pub(crate) fn mask_strings(text: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &b) in text.as_bytes().iter().enumerate() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == q {
                    quote = None;
                    continue;
                }
                if b != b'\n' {
                    out[i] = b' ';
                }
            }
            None => {
                if b == b'\'' || b == b'"' {
                    quote = Some(b);
                }
            }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Per byte: how many `{` are open before it.
fn depth_map(mask: &str) -> Vec<u32> {
    let mut depth = 0u32;
    let mut out = Vec::with_capacity(mask.len());
    for b in mask.bytes() {
        out.push(depth);
        match b {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    out
}

/// The value of a plain string literal (`'saved'`, `"saved"` without interpolation); anything else is `None`.
pub(crate) fn single_literal(text: &str) -> Option<String> {
    let text = text.trim();
    let (inner, quote) = if let Some(rest) = text
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        (rest, '\'')
    } else {
        let rest = text
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))?;
        (rest, '"')
    };
    let bad = inner.is_empty()
        || inner.contains([quote, '\\'])
        || (quote == '"' && inner.contains(['$', '{']));
    (!bad).then(|| inner.to_owned())
}

/// Every plain string literal in the text.
pub(crate) fn string_lits(text: &str) -> Vec<String> {
    re!(r#"'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)""#)
        .captures_iter(text)
        .filter_map(|caps| {
            let single = caps.get(1);
            let found = single.or_else(|| caps.get(2))?;
            let value = found.as_str();
            (single.is_some() || !value.contains('$')).then(|| value.to_owned())
        })
        .collect()
}

/// `Counter2` -> `counter2`, `UserTable` -> `user-table` (Laravel's `Str::kebab`).
pub(crate) fn kebab(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// The component name of a class file: the path below the `Livewire` directory, each part kebab-cased
/// (`app/Livewire/Admin/UserTable.php` -> `admin.user-table`).
fn comp_name(rel: &str) -> String {
    let path = rel.strip_suffix(".php").unwrap_or(rel);
    let parts: Vec<&str> = path.split('/').collect();
    let tail = match parts.iter().position(|part| *part == "Livewire") {
        Some(at) => &parts[at + 1..],
        None => &parts[parts.len().saturating_sub(1)..],
    };
    tail.iter()
        .map(|part| kebab(part))
        .collect::<Vec<_>>()
        .join(".")
}

/// An attribute of a member: its short name and the byte range between its parentheses.
#[derive(Debug)]
struct Attr {
    name: String,
    args: Option<(usize, usize)>,
}

/// The attributes inside `span` (the `#[...]` groups before a member).
fn attrs_of(clean: &str, span: (usize, usize)) -> Vec<Attr> {
    let mut out = Vec::new();
    let Some(slice) = clean.get(span.0..span.1) else {
        return out;
    };
    for group in re!(r"#\[(?:[^\[\]]|\[[^\]]*\])*\]").find_iter(slice) {
        let from = span.0 + group.start() + 2;
        let to = span.0 + group.end() - 1;
        for (a, b) in split_args(clean, from, to, Dialect::Php) {
            let item = &clean[a..b];
            let paren = item.find('(');
            let head = &item[..paren.unwrap_or(item.len())];
            let name = head.trim().rsplit('\\').next().unwrap_or("").to_owned();
            let args = paren.and_then(|p| {
                matching(clean, a + p, b, Dialect::Php).map(|close| (a + p + 1, close))
            });
            out.push(Attr { name, args });
        }
    }
    out
}

/// The braces of a body: from the `(` of a parameter list to the matching `{` .. `}`; `None` for an abstract
/// method (`;`).
pub(crate) fn body_of(mask: &str, paren_open: usize, limit: usize) -> Option<(usize, usize)> {
    let paren_close = matching(mask, paren_open, limit, Dialect::Php)?;
    let rest = mask.get(paren_close + 1..limit)?;
    let at = rest.find(['{', ';'])?;
    if rest.as_bytes()[at] != b'{' {
        return None;
    }
    let open = paren_close + 1 + at;
    let close = matching(mask, open, limit, Dialect::Php)?;
    Some((open, close))
}

/// One item of an array literal.
#[derive(Debug)]
pub(crate) struct Item {
    /// The key when it is a plain string literal.
    pub(crate) key: Option<String>,
    /// There is a `=>`.
    pub(crate) keyed: bool,
    pub(crate) val: String,
    pub(crate) spread: bool,
    pub(crate) at: usize,
}

/// The top-level items of the array literal opening at `open` (`[` or the `(` of `array(`).
pub(crate) fn array_items(clean: &str, open: usize) -> Option<Vec<Item>> {
    let close = matching(clean, open, clean.len(), Dialect::Php)?;
    let mut out = Vec::new();
    for (a, b) in split_args(clean, open + 1, close, Dialect::Php) {
        let text = &clean[a..b];
        if text.starts_with("...") {
            out.push(Item {
                key: None,
                keyed: false,
                val: String::new(),
                spread: true,
                at: a,
            });
            continue;
        }
        let parts = split_top(text, "=>", Dialect::Php);
        if parts.len() == 2 {
            out.push(Item {
                key: single_literal(parts[0]),
                keyed: true,
                val: parts[1].trim().to_owned(),
                spread: false,
                at: a,
            });
        } else {
            out.push(Item {
                key: None,
                keyed: false,
                val: text.to_owned(),
                spread: false,
                at: a,
            });
        }
    }
    Some(out)
}

/// The `[` / `(` of the array a declaration assigns (`= [` / `= array(`) after `from`.
fn array_open(mask: &str, from: usize) -> Option<usize> {
    let rest = mask
        .get(from..)?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let start = mask.len() - rest.len();
    if rest.starts_with('[') {
        return Some(start);
    }
    let after = rest.strip_prefix("array")?.trim_start();
    after.starts_with('(').then(|| mask.len() - after.len())
}

/// A rule value that contains the `required` rule (`'required|min:3'`, `['required', 'min:3']`).
fn is_required(value: &str) -> bool {
    string_lits(value)
        .iter()
        .any(|lit| lit.split('|').any(|rule| rule.trim() == "required"))
}

/// Adds a field, OR-ing the required flag.
fn add_rule(rules: &mut Vec<(String, bool)>, field: &str, required: bool) {
    match rules.iter_mut().find(|(name, _)| name == field) {
        Some((_, flag)) => *flag |= required,
        None => rules.push((field.to_owned(), required)),
    }
}

/// The fields of the rules array opening at `open`; an item without a literal key is a skipped one.
fn push_rules(
    clean: &str,
    open: usize,
    lines: &LineIndex,
    rules: &mut Vec<(String, bool)>,
    dynamic: &mut Vec<(&'static str, u32)>,
) {
    let Some(items) = array_items(clean, open) else {
        return;
    };
    for item in items {
        match (&item.key, item.spread) {
            (Some(key), false) => add_rule(rules, key, is_required(&item.val)),
            _ => dynamic.push(("lw-rules", lines.line_of(item.at))),
        }
    }
}

/// The event name of a `#[On('x')]` / `$listeners` literal: `Ok(None)` is not a Livewire event (`echo:` channels),
/// `Err` a name built at run time.
fn event_literal(lit: &str) -> Result<Option<String>, ()> {
    if lit.starts_with("echo") {
        return Ok(None);
    }
    if lit.contains(['{', '$']) {
        return Err(());
    }
    Ok(Some(lit.to_owned()))
}

/// The method whose line range holds the line.
fn method_at(ranges: &[(String, u32, u32)], line: u32) -> Option<String> {
    ranges
        .iter()
        .find(|(_, start, end)| *start <= line && line <= *end)
        .map(|(name, ..)| name.clone())
}

// ------------------------------------------------------------------------------------------------ PHP class

/// The Livewire component (or form) class of a PHP file; `None`: the file has none.
pub(super) fn scan_class(rel: &str, text: &str) -> Option<Class> {
    if !text.contains("Livewire") {
        return None;
    }
    let clean = strip_comments(text, Dialect::Php);
    let mask = mask_strings(&clean);
    let decl = re!(
        r"(?m)^[ \t]*(?:(?:final|abstract|readonly)\s+)*class\s+(?P<name>\w+)(?:\s+extends\s+(?P<parent>\\?[\w\\]+))?"
    )
    .captures(&mask)?;
    let name = decl.name("name")?.as_str().to_owned();
    let parent = decl.name("parent").map_or("", |m| m.as_str());
    let parent_short = parent.rsplit('\\').next().unwrap_or("");
    let form = match parent_short {
        "Component" => false,
        "Form" => true,
        _ => return None,
    };
    // the parent is the Livewire one: imported (`use Livewire\Component;`) or written out
    if !clean.contains(&format!("Livewire\\{parent_short}")) {
        return None;
    }
    let namespace = re!(r"(?m)^\s*namespace\s+([\w\\]+)\s*[;{]")
        .captures(&mask)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_owned());
    let fqn = match &namespace {
        Some(ns) => format!("{ns}\\{name}"),
        None => name.clone(),
    };
    let head_end = decl.get(0)?.end();
    let open = head_end + mask[head_end..].find('{')?;
    let close = matching(&mask, open, mask.len(), Dialect::Php)?;
    let depth = depth_map(&mask);
    let class_depth = depth[open] + 1;
    let lines = LineIndex::new(text);

    let mut out = Class {
        rel: rel.to_owned(),
        name: name.clone(),
        fqn,
        form,
        comp_name: comp_name(rel),
        line: lines.line_of(decl.get(0)?.start()),
        props: Vec::new(),
        methods: Vec::new(),
        complete: true,
        view: ViewRef::Convention,
        rules: Vec::new(),
        dispatches: Vec::new(),
        listens: Vec::new(),
        dynamic: Vec::new(),
    };
    let in_class = |at: usize| at > open && at < close && depth[at] == class_depth;
    let mut ranges: Vec<(String, u32, u32)> = Vec::new();
    let mut render_body: Option<(usize, usize)> = None;
    let mut rules_body: Option<(usize, usize)> = None;
    let mut has_render = false;

    for caps in re!(
        r"(?P<attrs>(?:#\[(?:[^\[\]]|\[[^\]]*\])*\]\s*)*)(?P<mods>(?:(?:public|protected|private|static|final|abstract)\s+)*)function\s+&?\s*(?P<name>\w+)\s*\("
    )
    .captures_iter(&mask)
    {
        let (Some(whole), Some(name_m), Some(mods)) =
            (caps.get(0), caps.name("name"), caps.name("mods"))
        else {
            continue;
        };
        if !in_class(whole.start()) {
            continue;
        }
        let attrs_span = caps.name("attrs").map_or((0, 0), |m| (m.start(), m.end()));
        let attrs = attrs_of(&clean, attrs_span);
        let method = name_m.as_str().to_owned();
        let line = lines.line_of(name_m.start());
        let body = body_of(&mask, whole.end() - 1, close);
        let end_line = body.map_or(line, |(_, end)| lines.line_of(end));
        ranges.push((method.clone(), line, end_line));
        out.methods.push(Meth {
            name: method.clone(),
            public: !mods.as_str().contains("protected") && !mods.as_str().contains("private"),
            computed: attrs.iter().any(|attr| attr.name == "Computed"),
        });
        for attr in attrs.iter().filter(|attr| attr.name == "On") {
            let lits = attr
                .args
                .map(|(a, b)| string_lits(&clean[a..b]))
                .unwrap_or_default();
            if lits.is_empty() {
                out.dynamic.push(("lw-event", line));
            }
            for lit in lits {
                match event_literal(&lit) {
                    Ok(Some(event)) => out.listens.push(Site {
                        name: event,
                        line,
                        method: Some(method.clone()),
                    }),
                    Ok(None) => {}
                    Err(()) => out.dynamic.push(("lw-event", line)),
                }
            }
        }
        match method.as_str() {
            "render" => {
                has_render = true;
                render_body = body;
            }
            "rules" => rules_body = body,
            "getListeners" => out.dynamic.push(("lw-listeners", line)),
            "__call" | "__get" => out.complete = false,
            _ => {}
        }
    }

    for caps in re!(
        r"(?P<attrs>(?:#\[(?:[^\[\]]|\[[^\]]*\])*\]\s*)*)(?P<mods>(?:(?:public|protected|private|static|readonly|var)\s+)+)(?:(?P<ty>[?\w\\|&]+)\s+)?\$(?P<name>\w+)"
    )
    .captures_iter(&mask)
    {
        let (Some(whole), Some(name_m), Some(mods)) =
            (caps.get(0), caps.name("name"), caps.name("mods"))
        else {
            continue;
        };
        if !in_class(whole.start()) || mods.as_str().contains("static") {
            continue;
        }
        let prop = name_m.as_str().to_owned();
        let attrs_span = caps.name("attrs").map_or((0, 0), |m| (m.start(), m.end()));
        let attrs = attrs_of(&clean, attrs_span);
        for attr in attrs
            .iter()
            .filter(|attr| matches!(attr.name.as_str(), "Validate" | "Rule"))
        {
            let Some((a, b)) = attr.args else { continue };
            let args = &clean[a..b];
            let start = args.trim_start();
            let keyed_open = a + (args.len() - start.len());
            let keyed = start.starts_with('[')
                && array_items(&clean, keyed_open)
                    .is_some_and(|items| items.iter().any(|item| item.keyed));
            if keyed {
                push_rules(
                    &clean,
                    keyed_open,
                    &lines,
                    &mut out.rules,
                    &mut out.dynamic,
                );
                continue;
            }
            // the rule literals stop at the first named option (`as:`, `message:`, ...)
            let mut cut = args.len();
            for hit in re!(r"\b(?:as|message|attribute|onUpdate|translate)\s*:").find_iter(args) {
                if !args[hit.end()..].starts_with(':') {
                    cut = hit.start();
                    break;
                }
            }
            let rule = &args[..cut];
            if !string_lits(rule).is_empty() {
                add_rule(&mut out.rules, &prop, is_required(rule));
            }
        }
        // the Livewire 2 `$listeners` and `$rules` properties
        if matches!(prop.as_str(), "listeners" | "rules")
            && let Some(array) = array_open(&mask, name_m.end())
        {
            if prop == "rules" {
                push_rules(&clean, array, &lines, &mut out.rules, &mut out.dynamic);
            } else if let Some(items) = array_items(&clean, array) {
                for item in items {
                    let line = lines.line_of(item.at);
                    let (event, handler) = match (&item.key, item.keyed) {
                        (Some(key), true) => (key.clone(), single_literal(&item.val)),
                        (None, false) => match single_literal(&item.val) {
                            Some(value) => (value.clone(), Some(value)),
                            None => {
                                out.dynamic.push(("lw-event", line));
                                continue;
                            }
                        },
                        _ => {
                            out.dynamic.push(("lw-event", line));
                            continue;
                        }
                    };
                    match event_literal(&event) {
                        Ok(Some(event)) => out.listens.push(Site {
                            name: event,
                            line,
                            method: handler,
                        }),
                        Ok(None) => {}
                        Err(()) => out.dynamic.push(("lw-event", line)),
                    }
                }
            }
        }
        let ty = caps
            .name("ty")
            .map(|m| m.as_str().trim_start_matches('?').to_owned());
        out.props.push(Prop {
            name: prop,
            public: mods.as_str().contains("public") || mods.as_str().contains("var"),
            url: attrs.iter().any(|attr| attr.name == "Url"),
            ty,
        });
    }

    // traits bring members that are not in the file
    for hit in re!(r"\buse\s+[\w\\]+(?:\s*,\s*[\w\\]+)*\s*[;{]").find_iter(&mask) {
        if in_class(hit.start()) {
            out.complete = false;
        }
    }
    if let Some((a, b)) = rules_body
        && let Some(hit) = re!(r"\breturn\s*(?:\[|array\s*\()").find(&mask[a..b])
    {
        push_rules(
            &clean,
            a + hit.end() - 1,
            &lines,
            &mut out.rules,
            &mut out.dynamic,
        );
    }
    for caps in re!(r"\$this\s*->\s*validate\s*\(\s*(\[)").captures_iter(&mask) {
        if let Some(open_at) = caps.get(1).map(|m| m.start())
            && open_at > open
            && open_at < close
        {
            push_rules(&clean, open_at, &lines, &mut out.rules, &mut out.dynamic);
        }
    }
    // the view
    if has_render {
        let literal = render_body.and_then(|(a, b)| {
            re!(r#"\bview\s*\(\s*['"]([\w.\-/]+)['"]"#)
                .captures(&clean[a..b])
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().replace('/', "."))
        });
        out.view = match literal {
            Some(view) => ViewRef::Named(view),
            None => ViewRef::Dynamic,
        };
    }
    // events: `$this->dispatch('x')`, `->dispatchTo('comp', 'x')`, Livewire 2 `emit*`
    for caps in re!(
        r"\$this\s*->\s*(?P<fn>dispatchTo|dispatchSelf|dispatch|emitTo|emitSelf|emitUp|emit)\s*\("
    )
    .captures_iter(&mask)
    {
        let (Some(whole), Some(func)) = (caps.get(0), caps.name("fn")) else {
            continue;
        };
        if whole.start() <= open || whole.start() >= close {
            continue;
        }
        let paren = whole.end() - 1;
        let Some(end) = matching(&mask, paren, close, Dialect::Php) else {
            continue;
        };
        let args = split_args(&clean, paren + 1, end, Dialect::Php);
        let index = usize::from(func.as_str().ends_with("To"));
        let Some(&(a, b)) = args.get(index) else {
            continue;
        };
        let line = lines.line_of(whole.start());
        match single_literal(&clean[a..b]) {
            Some(event) => out.dispatches.push(Site {
                name: event,
                line,
                method: method_at(&ranges, line),
            }),
            None => out.dynamic.push(("lw-event", line)),
        }
    }
    Some(out)
}

// ------------------------------------------------------------------------------------------------ Blade

/// `{{-- ... --}}` comments blanked (same length, newlines kept).
fn blank_blade_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for hit in re!(r"(?s)\{\{--.*?--\}\}").find_iter(text) {
        out.push_str(&text[last..hit.start()]);
        for ch in hit.as_str().chars() {
            if ch == '\n' {
                out.push('\n');
            } else {
                out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
            }
        }
        last = hit.end();
    }
    out.push_str(&text[last..]);
    out
}

/// The events of a template: `(dispatches, client listeners, lines of names that are not literals)`.
pub(super) type TemplateEvents = (Vec<(String, u32)>, Vec<(String, u32)>, Vec<u32>);

/// The events of the (comment-free) template text.
pub(super) fn template_events(clean: &str, lines: &LineIndex) -> TemplateEvents {
    let mut dispatches = Vec::new();
    let mut listens = Vec::new();
    let mut dynamic = Vec::new();
    for caps in re!(r"(?:\$wire\.|Livewire\.|\$)(?:dispatch|emit)(?P<kind>Self|Up|To)?\s*\(")
        .captures_iter(clean)
    {
        let Some(whole) = caps.get(0) else { continue };
        let paren = whole.end() - 1;
        let Some(end) = matching(clean, paren, clean.len(), Dialect::Php) else {
            continue;
        };
        let args = split_args(clean, paren + 1, end, Dialect::Php);
        let index = usize::from(caps.name("kind").is_some_and(|k| k.as_str() == "To"));
        let Some(&(a, b)) = args.get(index) else {
            continue;
        };
        let line = lines.line_of(whole.start());
        match single_literal(&clean[a..b]) {
            Some(event) => dispatches.push((event, line)),
            None => dynamic.push(line),
        }
    }
    for caps in re!(r#"(?:\$wire|Livewire)\.on\s*\(\s*['"]([^'"]+)['"]"#).captures_iter(clean) {
        if let (Some(whole), Some(event)) = (caps.get(0), caps.get(1)) {
            listens.push((event.as_str().to_owned(), lines.line_of(whole.start())));
        }
    }
    (dispatches, listens, dynamic)
}

/// Does the template talk to Livewire at all (otherwise `$dispatch` is plain Alpine).
pub(super) fn mentions_livewire(text: &str) -> bool {
    text.contains("wire:")
        || text.contains("<livewire:")
        || text.contains("@livewire")
        || text.contains("Livewire.")
}

/// The events of a template text (comments are blanked first).
pub(super) fn events_of_text(text: &str) -> TemplateEvents {
    let clean = blank_blade_comments(text);
    template_events(&clean, &LineIndex::new(&clean))
}

/// The `wire:` events whose value is an action call.
const ACTION_EVENTS: [&str; 26] = [
    "click",
    "dblclick",
    "submit",
    "keydown",
    "keyup",
    "keypress",
    "change",
    "input",
    "blur",
    "focus",
    "focusin",
    "focusout",
    "mouseenter",
    "mouseleave",
    "mouseover",
    "mouseout",
    "mousedown",
    "mouseup",
    "scroll",
    "poll",
    "init",
    "intersect",
    "paste",
    "copy",
    "cut",
    "contextmenu",
];

/// What a Blade template says about components, bindings and events.
pub(super) fn scan_view(text: &str) -> ViewScan {
    let clean = blank_blade_comments(text);
    let lines = LineIndex::new(&clean);
    let mut out = ViewScan::default();
    for caps in re!(r"<livewire:(?P<name>[\w.\-:]+)").captures_iter(&clean) {
        let (Some(whole), Some(found)) = (caps.get(0), caps.name("name")) else {
            continue;
        };
        let name = found.as_str().trim_end_matches(['.', ':']);
        let line = lines.line_of(whole.start());
        if name.contains("::") {
            continue;
        }
        if matches!(name, "is" | "dynamic-component") {
            out.dynamic.push(("lw-component", line));
            continue;
        }
        out.uses.push(UseRef {
            target: Target::Name(name.to_owned()),
            line,
        });
    }
    for hit in re!(r"@livewire\s*\(").find_iter(&clean) {
        let open = hit.end() - 1;
        let line = lines.line_of(hit.start());
        let Some(close) = matching(&clean, open, clean.len(), Dialect::Php) else {
            continue;
        };
        let Some(&(a, b)) = split_args(&clean, open + 1, close, Dialect::Php).first() else {
            continue;
        };
        let first = &clean[a..b];
        if let Some(name) = single_literal(first) {
            if !name.contains("::") {
                out.uses.push(UseRef {
                    target: Target::Name(name),
                    line,
                });
            }
        } else if let Some(class) = first.trim().strip_suffix("::class")
            && !class.is_empty()
            && class
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '\\')
        {
            out.uses.push(UseRef {
                target: Target::Class(class.trim_start_matches('\\').to_owned()),
                line,
            });
        } else {
            out.dynamic.push(("lw-component", line));
        }
    }
    for caps in re!(
        r#"\bwire:(?P<ev>[A-Za-z][\w\-]*)(?:\.[\w\-:]+)*\s*=\s*(?:"(?P<dq>[^"]*)"|'(?P<sq>[^']*)')"#
    )
    .captures_iter(&clean)
    {
        let (Some(whole), Some(event)) = (caps.get(0), caps.name("ev")) else {
            continue;
        };
        let value = caps
            .name("dq")
            .or_else(|| caps.name("sq"))
            .map_or("", |m| m.as_str())
            .trim();
        let bind = Bind {
            attr: format!("wire:{}", event.as_str()),
            value: value.to_owned(),
            line: lines.line_of(whole.start()),
        };
        if event.as_str() == "model" {
            out.models.push(bind);
        } else if ACTION_EVENTS.contains(&event.as_str()) {
            out.actions.push(bind);
        }
    }
    let (dispatches, listens, dynamic) = template_events(&clean, &lines);
    out.dispatches = dispatches;
    out.listens = listens;
    out.dynamic
        .extend(dynamic.into_iter().map(|line| ("lw-event", line)));
    out
}

// ------------------------------------------------------------------------------------------------ routes

/// The `Route::get('/x', X::class)` registrations of a route file.
pub(super) fn scan_routes(rel: &str, text: &str) -> Vec<RouteUse> {
    if !text.contains("::class") {
        return Vec::new();
    }
    let clean = strip_comments(text, Dialect::Php);
    let lines = LineIndex::new(text);
    let uses: Vec<(String, String)> =
        re!(r"(?m)^\s*use\s+(?P<fqn>[\w\\]+)(?:\s+as\s+(?P<alias>\w+))?\s*;")
            .captures_iter(&clean)
            .filter_map(|caps| {
                let fqn = caps
                    .name("fqn")?
                    .as_str()
                    .trim_start_matches('\\')
                    .to_owned();
                let alias = match caps.name("alias") {
                    Some(alias) => alias.as_str().to_owned(),
                    None => fqn.rsplit('\\').next()?.to_owned(),
                };
                Some((alias, fqn))
            })
            .collect();
    let mut out = Vec::new();
    for caps in re!(
        r#"Route::(?P<verb>get|post|put|patch|delete|any)\s*\(\s*(?:'(?P<u1>[^']*)'|"(?P<u2>[^"]*)")\s*,\s*(?P<cls>\\?[\w\\]+)::class\s*\)"#
    )
    .captures_iter(&clean)
    {
        let (Some(whole), Some(verb), Some(class)) =
            (caps.get(0), caps.name("verb"), caps.name("cls"))
        else {
            continue;
        };
        let uri = caps
            .name("u1")
            .or_else(|| caps.name("u2"))
            .map_or("", |m| m.as_str());
        let written = class.as_str().trim_start_matches('\\');
        let class = if written.contains('\\') {
            written.to_owned()
        } else {
            uses.iter()
                .find(|(alias, _)| alias == written)
                .map_or_else(|| written.to_owned(), |(_, fqn)| fqn.clone())
        };
        let chain = &clean[whole.end()..];
        let chain = &chain[..chain.find(';').unwrap_or(chain.len())];
        let name = re!(r#"->name\(\s*['"]([^'"]+)['"]"#)
            .captures(chain)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_owned());
        out.push(RouteUse {
            file: rel.to_owned(),
            line: lines.line_of(whole.start()),
            verb: verb.as_str().to_ascii_uppercase(),
            uri: uri.to_owned(),
            class,
            name,
        });
    }
    out
}
