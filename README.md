# rust-competitive-helper
How to use it:
- Switch default rust toolchain to nightly: 
    ```
    rustup default nightly
    ```
- Install rust-competitive-helper binary:
    ```
    cargo install --git https://github.com/rust-competitive-helper/rust-competitive-helper
    ```
- Fork [example-contests-workspace](https://github.com/rust-competitive-helper/example-contests-workspace) repository on github, clone it locally, open in CLion
- In CLion terminal run `rust-competitive-helper` from current directory

To use with [Competitive Companion](https://github.com/jmerle/competitive-companion):
- Add 4244 to custom ports in plugin
- Choose "Run listener" in `rust-competitive-helper`
- Click "Parse task" in plugin
- Project for this task will be created and opened in CLion.
- Testing should be done by running main.rs in corresponding crate
- Submit ./main/src/main.rs

# Config
There is a config file, which is automatically created on the first run.

Default locations:
- Linux:   /home/alice/.config/rust-competitive-helper
- Windows: C:\Users\Alice\AppData\Roaming\Foo Corp\rust-competitive-helper
- macOS:   /Users/Alice/Library/Preferences/rust-competitive-helper
 
By default CLion is used to open newly created task, but you can 
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
