//! Jegyzetek (PILOT): rövid szöveg egy fájlhoz vagy metódushoz, a forráson KÍVÜL tárolva
//! (`<projekt>/.callgraph/notes.jsonl`). A `show` outline / `--body` kimenete a horgony alatt mutatja,
//! a `show --notes <minta>` és a `callgraph note list` keres / listáz. A horgony NÉV szerinti (nem sorszám):
//! ha a metódus eltűnik a fájlból, a jegyzet „árva” lesz. Leírás: `docs/NOTES.md`.

mod note;
mod store;
mod view;

pub use note::{FORMAT_VERSION, Note, short_time, truncate};
pub use store::{NOTES_FILE, NoteStore};
pub use view::{
    NoteFilter, annotate, body_lines, file_lines, list_line, method_lines, orphaned, relative_path,
    search_text, unlisted_lines,
};
