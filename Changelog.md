**2026-05-21** Task metadata moved out of `main.rs` into a sibling
`tasks/<name>/task.json`, and the assembled build now produces
`main/task.json` instead of writing `// <url>` on the first line of
`main/src/main.rs`. Legacy first-line `//<json>` is still read as a
fallback, so existing tasks keep working until they are recreated or
rebuilt. Templates that include `//$JSON` on their first line will
have that line stripped during task creation; you may wish to remove
it from the template too.

**2026-05-20** Every menu action is now available as a CLI subcommand:
`rust-competitive-helper submit`, `... new <name> [flags]`,
`... archive <contest>` (or `--task NAME`), and `... help`. Running
with no arguments still launches the interactive menu.

**2026-05-19** kep.uz submissions are now routed through
[submitter](https://github.com/EgorKulikov/submitter).

**2026-04-22** Improved WSL integration: when a Windows IDE
(`rustrover.cmd` / `.exe` / `.bat`) is launched from a Linux or WSL
working directory, the `$FILE` argument is rewritten to an absolute
Windows path (via `wslpath -w` on Linux, or by joining with the CWD on
Windows). JetBrains IDEs no longer try to open a non-existent file
under `C:\Windows`.

**2026-04-14** Code minimization now skips `main.rs`. Only library code
is minimized; the solution stays readable.

**2026-03-25** Re-added luogu support.

**2026-03-20** AtCoder support added via submitter.

**2026-03-19** Submissions now go through the standalone
[submitter](https://github.com/EgorKulikov/submitter) binary for every
supported judge; the in-repo per-site adapters were removed. Install
`submitter` separately to keep submitting from inside
rust-competitive-helper.

**2025-03-07** On Linux, the default editor is now RustRover (used to be
CLion). The `Submit` action falls back to copying the assembled source
to the clipboard when no integration succeeds. Luogu support was
temporarily dropped because of Cloudflare protection (restored on
2026-03-25).

**2024-12-23** New `syn`-based file parser replaces the hand-rolled
build pipeline; submitter support was introduced and the per-site
submission code was split out of this crate.

**2024-11-16** A lot of breaking changes. To use it, you need:

- sudo apt install libxcb-shape0-dev libxcb-xfixes0-dev
- update templates/Cargo.toml to add extra "../" for algo_lib and rust_competitive_helper_util paths
- tester/helper.rs needs to handle properly that all tasks are created under ./tasks/

**2022-09-30** Improve the support of multi-file solutions. Save all files when archiving the task.

**2022-09-30** Support minimization of the generated code. Currently it just trims all spaces in each line. It is tunred off by default. To use it, in `build.rs` file, use code like:

```
fn main() {
    rust_competitive_helper_util::build::build_several_libraries(&["algo_lib".to_owned()], true);
}
```
