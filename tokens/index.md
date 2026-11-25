> Why Claude Code burns tokens on Grep and whole-file reads, and how a call graph code map finds the right method in a few lines. Free, MIT.

Source: https://callgraph.eu/tokens/

# Reduce Claude Code token usage
    Most of the tokens a coding agent spends are not spent on writing code. They go to finding it:
      Grep hits, whole files read to check one function, the same file read again ten minutes later.
      callgraph is a free call graph and code map that lets Claude Code open only the lines it needs.

      [Install](https://callgraph.eu/#install)
      [Use it over MCP](https://callgraph.eu/mcp/)

  ## Why an agent burns tokens on navigation

    - **Grep returns every hit.** A common name matches dozens of lines in files that have nothing to do with the task, and each line is paid for.
    - **Whole files get read.** To check one method the agent opens the entire file, often a thousand lines, and most of it is never used.
    - **The same code is read again.** After a long session or a context reset the agent has forgotten what it saw and reads it a second time.
    - **Guessing costs a round trip.** A wrong file means a wasted read, and then another guess.

  All of this is context the model has to carry for the rest of the session, so the cost keeps growing after the search is over.

  ## How callgraph replaces that
  callgraph parses your code into an AST index once and answers four questions, each in a few lines.

    - **Where do I start?** `locate` takes the names from the task and ranks the few files worth opening, with the reason for each.
    - **What is in this file?** The outline lists classes and methods with their line ranges and what each one calls.
    - **Show me one method.** `file::method --body` prints just that method, with line numbers.
    - **Who calls it?** `--grep` finds the callers, so you see what a change would touch.

```
CG=~/.claude/skills/callgraph/callgraph.sh
bash $CG locate refund --project shop                          # 1. which files matter
bash $CG --project shop app/Services/RefundService.php         # 2. outline with line ranges
bash $CG --project shop app/Services/RefundService.php::processRefund --body   # 3. one method
bash $CG --project shop app --grep '(^|::|\.|->)processRefund$'                # 4. callers
```
  Output of steps 1 and 2 (shortened)
```
locate: 1 terms · 6 matches in 3 files · start here ↓
1. app/Services/RefundService.php  ★★★
   processRefund()  L6-19  callers: 2  ← "refund" (exact method)
2. app/Http/Controllers/RefundController.php  ★★
   store()  L14-22  calls RefundService::processRefund

FILE  app/Services/RefundService.php
class RefundService  L5-41
  processRefund()  L6-19
  notifyCustomer()  L21-30
  logRefund()  L32-40
```
  The agent reads about a dozen lines to find the file, then a method of fifteen lines instead of a file of four hundred.
    The same flow works for PHP, JavaScript / TypeScript, Vue, React, Rust, Swift, Python, Go and Kotlin.
    [More about locate](https://callgraph.eu/locate/).

  ## Set it up in three steps

    - **Download and install.** Run the one-line installer from the [home page](https://callgraph.eu/#install). It unpacks the Claude Code skill and one binary; only `bash` is required.
    - **Run the installer script.** `bash ~/.claude/skills/callgraph/install.sh` registers your project and writes a one-sentence rule file, `.claude/rules/callgraph.md`, so Claude Code uses callgraph on its own.
    - **Or use the MCP server.** If you prefer tools over a skill, register `callgraph mcp` and the same queries appear as tools. See [MCP server setup](https://callgraph.eu/mcp/).

```
bash ~/.claude/skills/callgraph/install.sh
```
  Then restart Claude Code and give it a normal task. There is nothing to configure per prompt.

  ## Cursor, Codex and VS Code too
  The skill is for Claude Code. Every other agent uses the MCP server: the same `locate`, outline, method-body and
    caller tools, running locally on your machine. Setup for Claude Desktop, Cursor, VS Code and Codex is on the
    [MCP page](https://callgraph.eu/mcp/). The code never leaves your computer.

  ## Questions
  ### Does callgraph change my code?
No. It only reads your project and answers questions about it: where something is, what it calls and who calls it. Nothing is written to your source files.
  ### Do I have to change how I prompt Claude Code?
No. After `install.sh` writes the one-sentence rule file, Claude Code reaches for callgraph on its own when a task starts. You can still ask for it by name.
  ### Does it work with Cursor, Codex and VS Code?
Yes. Run callgraph as a local MCP server and any MCP client gets the same tools: locate, outline, method bodies and callers.
  ### Which languages are supported?
PHP, JavaScript / TypeScript, Vue, React, Rust, Swift, Python, Go and Kotlin, plus the structure of CSS, JSON, Markdown and HTML files.

  ## Related

    [### locateRanked files to start a task with.](https://callgraph.eu/locate/)
    [### MCP serverThe same queries in any MCP client.](https://callgraph.eu/mcp/)
    [### Project profileHow an unfamiliar project is built, in about 15 lines.](https://callgraph.eu/profile/)
    [### InstallOne line for macOS, Linux and Windows.](https://callgraph.eu/#install)
