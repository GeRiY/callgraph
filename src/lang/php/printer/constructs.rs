use mago_syntax::cst::{ArgumentList, Construct, Match, MatchArm};

use super::Printer;

impl Printer<'_, '_> {
    pub(super) fn construct(&mut self, c: &Construct<'_>) -> String {
        match c {
            Construct::Isset(x) => format!(
                "isset({})",
                x.values
                    .iter()
                    .map(|e| self.expr(e))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Construct::Empty(x) => format!("empty({})", self.expr(x.value)),
            Construct::Eval(x) => format!("eval({})", self.expr(x.value)),
            Construct::Include(x) => format!("include {}", self.expr(x.value)),
            Construct::IncludeOnce(x) => format!("include_once {}", self.expr(x.value)),
            Construct::Require(x) => format!("require {}", self.expr(x.value)),
            Construct::RequireOnce(x) => format!("require_once {}", self.expr(x.value)),
            Construct::Print(x) => format!("print {}", self.expr(x.value)),
            Construct::Exit(x) => self.exit("exit", x.arguments.as_ref()),
            Construct::Die(x) => self.exit("die", x.arguments.as_ref()),
        }
    }

    fn exit(&mut self, word: &str, args: Option<&ArgumentList<'_>>) -> String {
        match args {
            Some(a) if !a.arguments.is_empty() => format!("{word}({})", self.args(a)),
            _ => word.to_string(),
        }
    }

    pub(super) fn match_expr(&mut self, m: &Match<'_>) -> String {
        let cond = self.expr(m.expression);
        self.indent += 1;
        let mut out = String::new();
        for arm in m.arms.iter() {
            let (head, body) = match arm {
                MatchArm::Expression(a) => {
                    let conds: Vec<_> = a.conditions.iter().map(|c| self.expr(c)).collect();
                    (conds.join(", "), a.expression)
                }
                MatchArm::Default(a) => ("default".to_string(), a.expression),
            };
            out.push_str(&format!("{}{head} => {},", self.nl(), self.expr(body)));
        }
        self.indent -= 1;
        format!("match ({cond}) {{{out}{}}}", self.nl())
    }
}
