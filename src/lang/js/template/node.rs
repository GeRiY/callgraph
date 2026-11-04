//! A template toleráns szintaxisfája (a `@vue/compiler-core` parse-fa leegyszerűsített mása).

/// Egy elem- vagy szövegcsomópont.
#[derive(Debug)]
pub enum Node {
    Element(Element),
    /// `{{ kifejezés }}` (a tartalom már vágott és dekódolt).
    Interpolation {
        line: u32,
        content: String,
    },
    /// Nyers szöveg (dekódolatlan), a kezdősorával.
    Text {
        line: u32,
        raw: String,
        decode: bool,
    },
}

#[derive(Debug)]
pub struct Element {
    pub tag: String,
    pub line: u32,
    pub end_line: u32,
    pub props: Vec<Prop>,
    pub children: Vec<Node>,
}

/// Elem-attribútum: statikus vagy direktíva.
#[derive(Debug)]
pub enum Prop {
    Attr {
        line: u32,
        value: Option<String>,
    },
    Dir {
        line: u32,
        /// `if`, `on`, `bind`, `slot`, ...
        name: String,
        arg: Option<String>,
        exp: Option<String>,
    },
}
