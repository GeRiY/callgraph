//! Text checks on a component's script: is its prop set statically known (so an attribute the child does not
//! declare is a real finding, not a fall-through or a mixin prop)?

use crate::lang::js::{Sfc, SfcBlockKind};

/// The text of the `<script>` / `<script setup>` blocks, joined.
pub(super) fn script_text(text: &str, sfc: &Sfc) -> String {
    sfc.scripts()
        .filter_map(|block| text.get(block.start..block.end))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `<script lang="tsx|jsx">`: the template is not the whole story.
pub(super) fn has_jsx(sfc: &Sfc) -> bool {
    sfc.blocks.iter().any(|block| {
        block.kind != SfcBlockKind::Template && matches!(block.lang.as_deref(), Some("tsx" | "jsx"))
    })
}

/// Does the script pass attributes on (`$attrs`, `useAttrs`, `inheritAttrs`) or inherit props (`mixins`,
/// `extends`)? Then an unknown attribute may well be consumed.
pub(super) fn opens_props(script: &str) -> bool {
    [
        "$attrs",
        "useAttrs",
        "inheritAttrs",
        "mixins",
        "extends:",
        "extends :",
    ]
    .iter()
    .any(|needle| script.contains(needle))
}

/// Is every declared prop visible to the analysis: an inline `defineProps<{ … }>()`, a local `interface` /
/// `type` literal without `extends` / `&`, or a runtime `defineProps({ … })` / `props: { … }` / `props: [ … ]`
/// without spreads. An imported or composed type (`Omit<…>`, `A & B`) is not.
pub(super) fn props_complete(script: &str) -> bool {
    if opens_props(script) {
        return false;
    }
    if let Some(at) = script.find("defineProps<") {
        return type_arg_complete(script, &script[at + "defineProps<".len()..]);
    }
    for head in ["defineProps(", "props:", "props :"] {
        if let Some(at) = script.find(head) {
            let rest = script[at + head.len()..].trim_start();
            return match rest.chars().next() {
                Some(open @ ('{' | '[')) => {
                    balanced(rest, open).is_some_and(|end| !rest[..end].contains("..."))
                }
                _ => false,
            };
        }
    }
    false
}

fn type_arg_complete(script: &str, arg: &str) -> bool {
    let arg = arg.trim_start();
    if arg.starts_with('{') {
        return balanced(arg, '{').is_some_and(|end| arg[end..].trim_start().starts_with('>'));
    }
    let ident: String = arg
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() || !arg[ident.len()..].trim_start().starts_with('>') {
        return false;
    }
    local_literal(script, &ident)
}

/// `interface Ident {` (no `extends`) or `type Ident = { … }` (nothing composed after the literal).
fn local_literal(script: &str, ident: &str) -> bool {
    let interface = format!("interface {ident}");
    if let Some(at) = script.find(&interface) {
        let rest = script[at + interface.len()..].trim_start();
        // a longer name (`interface PropsBase`) is another declaration
        let exact =
            !script[at + interface.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
        return exact && rest.starts_with('{');
    }
    let alias = format!("type {ident}");
    if let Some(at) = script.find(&alias) {
        let after = &script[at + alias.len()..];
        let exact = !after.starts_with(|c: char| c.is_alphanumeric() || c == '_');
        let rest = after.trim_start();
        let Some(literal) = rest.strip_prefix('=').map(str::trim_start) else {
            return false;
        };
        return exact
            && literal.starts_with('{')
            && balanced(literal, '{')
                .is_some_and(|end| !literal[end..].trim_start().starts_with(['&', '|']));
    }
    false
}

/// The byte offset just after the bracket that closes the one `text` starts with.
fn balanced(text: &str, open: char) -> Option<usize> {
    let close = if open == '{' { '}' } else { ']' };
    let mut depth = 0usize;
    for (at, c) in text.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at + c.len_utf8());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_and_local_types_are_complete() {
        assert!(props_complete("defineProps<{ a: string }>()"));
        assert!(props_complete(
            "interface Props { a: string }\nconst p = defineProps<Props>()"
        ));
        assert!(props_complete(
            "type Props = { a: string };\nwithDefaults(defineProps<Props>(), {})"
        ));
        assert!(props_complete(
            "defineProps({ a: String, b: { type: Number } })"
        ));
        assert!(props_complete("export default { props: ['a', 'b'] }"));
    }

    #[test]
    fn composed_imported_or_opened_props_are_not() {
        assert!(!props_complete(
            "import type { Props } from './t'\ndefineProps<Props>()"
        ));
        assert!(!props_complete(
            "interface Props extends Base { a: string }\ndefineProps<Props>()"
        ));
        assert!(!props_complete(
            "type Props = Base & { a: string }\ndefineProps<Props>()"
        ));
        assert!(!props_complete("defineProps<Omit<Base, 'a'>>()"));
        assert!(!props_complete(
            "defineProps<{ a: string }>(); const attrs = useAttrs()"
        ));
        assert!(!props_complete("defineProps({ ...shared, a: String })"));
        assert!(!props_complete(
            "export default { mixins: [m], props: ['a'] }"
        ));
        assert!(!props_complete("const x = 1"));
    }
}
