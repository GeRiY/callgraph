//! A fő (module / component) container `doc`-ja: blokkonként, amíg üres — az első utasítás docblockja,
//! különben a blokk első, a blokk elején álló blokk-kommentje (a régi motor `analyzeProgram` eleje).

use oxc_ast::ast::Program;
use oxc_span::GetSpan;

use super::analysis::{JsAnalysis, JsPass, PassContext};
use super::methods::{Docs, container_doc};

pub struct ContainerDocPass;

impl JsPass for ContainerDocPass {
    fn run(&self, ctx: &PassContext<'_>, program: &Program<'_>, out: &mut JsAnalysis) {
        if !out.builder.first_container_doc_is_none() {
            return;
        }
        let docs = Docs {
            source: ctx.source,
            comments: &program.comments,
        };
        let first = program.body.first().map(|s| s.span().start);
        let doc = container_doc(&docs, first, ctx.lines);
        out.builder.set_first_container_doc(doc);
    }
}
