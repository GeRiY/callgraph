//! Receiver typing of Go calls through the caller index: the receiver parameter, locals from multi-value calls
//! (by position) and method chains resolve to `Type::method`.

use crate::core::SourceFile;
use crate::index::CallerIndex;
use crate::lang::Language;
use crate::lang::go::GoFrontend;
use crate::model::FileGraph;

fn graph(text: &str) -> FileGraph {
    GoFrontend::with_defaults()
        .analyze(&SourceFile::new(
            "/p/cmd.go",
            text.as_bytes().to_vec(),
            None,
        ))
        .unwrap()
        .graph
}

const SRC: &str = r#"package cobra

type Command struct {
	parent *Command
}

type Other struct{}

func (c *Command) Find(args []string) (*Command, []string, error) {
	return c, args, nil
}

func (c *Command) Root() *Command {
	return c
}

func (c *Command) execute(flags []string) error {
	return nil
}

func (c *Command) ExecuteC() error {
	cmd, flags, err := c.Find(nil)
	if err != nil {
		return err
	}
	return cmd.execute(flags)
}

func (c *Command) Chain() error {
	return c.Root().execute(nil)
}

func (c *Command) Direct() error {
	return c.execute(nil)
}

func (c *Command) Up() error {
	return c.parent.execute(nil)
}

func Helper(d *Command) error {
	return d.execute(nil)
}

func (o *Other) execute() {}

func (o *Other) Run() {
	o.execute()
}

func (c *Command) Unknown(x Thing) {
	x.execute(nil)
}
"#;

fn callers_of(index: &CallerIndex, pattern: &str) -> Vec<String> {
    let mut names: Vec<String> = index
        .callers(pattern, 1)
        .unwrap()
        .into_iter()
        .map(|row| row.caller)
        .collect();
    names.sort();
    names
}

#[test]
fn receiver_param_multi_return_local_and_chain_resolve_to_the_type() {
    let g = graph(SRC);
    let index = CallerIndex::build([("/p/cmd.go", &g)]);
    // `Other.Run` calls `Other::execute` and `Unknown` calls `Thing::execute`: neither is `Command::execute`
    assert_eq!(
        callers_of(&index, "Command::execute"),
        vec![
            "Chain".to_owned(),
            "Direct".to_owned(),
            "ExecuteC".to_owned(),
            "Helper".to_owned(),
            "Up".to_owned(),
        ]
    );
    assert_eq!(callers_of(&index, "Other::execute"), vec!["Run".to_owned()]);
}

#[test]
fn multi_return_local_gets_its_type_by_position() {
    let g = graph(SRC);
    let call = g
        .calls
        .iter()
        .find(|c| c.caller.as_deref() == Some("ExecuteC") && c.target == "cmd.execute")
        .expect("cmd.execute call");
    assert_eq!(call.resolved.as_deref(), Some("cobra\\Command::execute"));
}
