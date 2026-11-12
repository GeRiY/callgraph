//! `callgraph usage-check --background <query>` — a napi névtelen check elküldése leválasztott gyerekfolyamatból
//! (a `wrap` indítja, naponta legfeljebb egyszer; lásd `update::usage`). Belső parancs: kimenete nincs, hibát
//! nem ad, és csak a `v=&p=&via=&n=&w=&m=` alakú query-t küldi el (más szöveget nem).

use std::io::Write;

use crate::cli::Command;
use crate::error::Result;
use crate::update::{BACKGROUND_TIMEOUT, HttpFetcher};

pub struct UsageCheckCommand {
    args: Vec<String>,
}

impl UsageCheckCommand {
    pub fn new(args: Vec<String>) -> Self {
        Self { args }
    }
}

/// Csak a check saját query-je mehet ki: ismert kulcsok, rövid, egyszerű értékek.
fn valid_query(query: &str) -> bool {
    query.len() <= 200
        && query.split('&').all(|pair| {
            pair.split_once('=').is_some_and(|(k, v)| {
                matches!(k, "v" | "p" | "via" | "n" | "w" | "m")
                    && !v.is_empty()
                    && v.chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            })
        })
}

impl Command for UsageCheckCommand {
    fn run(&self, _out: &mut dyn Write, _err: &mut dyn Write) -> Result<i32> {
        if let [flag, query] = self.args.as_slice()
            && flag == "--background"
            && valid_query(query)
        {
            crate::update::usage::send_now(&HttpFetcher::new(BACKGROUND_TIMEOUT), query);
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_check_query_passes() {
        assert!(valid_query("v=0.9.4&p=linux-x86_64&via=cli&n=1&w=0&m=1"));
        assert!(valid_query(
            "v=0.9.4-dev.abc1234&p=darwin-arm64&via=mcp&n=0&w=0&m=0"
        ));
        for bad in [
            "",
            "v=0.9.4&id=123",
            "v=0.9.4&p=/home/user",
            "v=a b",
            "v=0.9.4&p=x&via=cli&n=1&w=0&m=1&x=1",
            "v=",
            &"v=1&".repeat(60),
        ] {
            assert!(!valid_query(bad), "{bad}");
        }
    }
}
