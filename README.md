# rust-competitive-helper
How to use it:
- Install extra libraries:
  ```
  sudo apt install libxcb-shape0-dev libxcb-xfixes0-dev
  ```
- Install rust-competitive-helper binary:
    ```
    cargo install --git https://github.com/rust-competitive-helper/rust-competitive-helper
    ```
- Fork [example-contests-workspace](https://github.com/rust-competitive-helper/example-contests-workspace) repository on github, clone it locally, open in RustRover
- In the IDE terminal run `rust-competitive-helper` from the project root

To use with [Competitive Companion](https://github.com/jmerle/competitive-companion):
- Add 4244 to custom ports in plugin
- Start `rust-competitive-helper` (the listener starts automatically)
- Click "Parse task" in plugin
- A task crate will be created and the solution file opened in your IDE
- Testing should be done by running main.rs in corresponding crate
- To submit, pick "Submit" in the `rust-competitive-helper` menu — it dispatches to [submitter](https://github.com/EgorKulikov/submitter) for supported judges (Codeforces, AtCoder, kep.uz, etc.) or copies the assembled `main/src/main.rs` to the clipboard for unsupported sites

# Config
`config.toml` is created in the project root on the first run. If an older
global config exists (from previous versions), its contents are migrated
once:
- Linux:   `~/.config/rust-competitive-helper/default-config.toml`
- Windows: `%APPDATA%\rust-competitive-helper\default-config.toml`

By default RustRover is used to open newly created tasks, but you can
override it to use vscode for example:
```
open_task_command = [
    '/usr/bin/code',
    '-r',
    '--goto',
    '$FILE:$LINE:$COLUMN',
]
```

# Other stuff

To make git not track changes in auto-generated main.rs file:
```
git update-index --assume-unchanged main/src/main.rs
```

If you want to use your version of rust-contest-helper, you can run it like this:
```
RUST_BACKTRACE=1 cargo run --manifest-path ../rust-competitive-helper/Cargo.toml 
```

If bug was fixed in this library, and you want cargo to start use new version, run:
```
cargo update
```
