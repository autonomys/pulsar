<div align="center">

# Pulsar

Pulsar simplifies the farming process on Subspace Network.

[![ci-tests](https://img.shields.io/github/actions/workflow/status/subspace/pulsar/ci-tests.yml?branch=main&label=CI&logo=github&style=for-the-badge)](https://github.com/subspace/pulsar/actions/workflows/ci-tests.yml)
[![Rust Docs](https://img.shields.io/github/actions/workflow/status/subspace/pulsar/rustdoc.yml?branch=main&label=RUST-DOCS&logo=github&style=for-the-badge)](https://github.com/subspace/pulsar/actions/workflows/rustdoc.yml)
[![Latest Release](https://img.shields.io/github/v/release/subspace/pulsar?include_prereleases&logo=github&style=for-the-badge)](https://github.com/subspace/pulsar/releases)

![prompt](images/pulsar-prompt.png)

</div>

---

Instead of running a terminal instance for the farmer, and running another terminal instance for the node, now you can run a SINGLE terminal instance to farm!

## How to Use (commands)

1. Download the executable from [releases](https://github.com/subspace/pulsar/releases)
2. In your terminal, change your directory to where you download the file for example: if you downloaded your file to your `Downloads` folder, `cd Downloads`.
3. We will address your executable name as `pulsar`, change the below commands accordingly to your full executable name.
4. Run `./pulsar init` -> this will initialize your config file, which will store the necessary information for you to farm.
5. Run `./pulsar farm` -> this will start farming. Yes, it is that simple! Enjoy! 🎉

## Other commands

- `wipe` -> This is a dangerous one. If you want to delete everything and start over, this will permanently delete your plots and your node data (this will not erase any rewards you have gained, don't worry).
- `info` -> This will show info for your farming.

## Daemonizing the Process (Moving it to the Background)

In some instances, you may want to move the farming process to the background. Tools like [`screen`](https://www.gnu.org/software/screen/manual/screen.html) and [`tmux`](https://github.com/tmux/tmux) can help manage this.

![Alt text](images/culture.jpeg)

### Example with `tmux`

```sh
$ tmux -S farming
```

This will create a new `tmux` session using a socket file named `farming`.

Once the tmux session is created, you can go ahead and run the farming process.

```sh
$ ./pulsar farm
```

Once it's running, you can detach the process by pressing `CTRL+b d` (read more about [detaching a sessions](https://linuxhint.com/detach-session-tmux/))

That's it, you should be back to your terminal, with _subspace farming_ running in the background as a session.

To re-attach to your session, use tmux:

```sh
$ tmux -S farming attach
```

If you ever want to delete/kill your farming session, enter the command:

```sh
tmux kill-session -t farming
```

### Example with `screen`

```sh
screen -S farming
```

This will create a new `screen` session.

```sh
./pulsar farm
```

Once it's running, you can detach the process by pressing `CTRL+d a`.

To re-attach it to your current session:

```sh
screen -r farming
```

If you ever want to delete/kill your farming session, enter the command:

```sh
screen -S farming -X quit
```

## Developer

### Pre-requisites

You'll have to have [Rust toolchain](https://rustup.rs/) installed as well as LLVM, Clang and CMake in addition to usual developer tooling.

Below are some examples of how to install these dependencies on different operating systems.

#### Ubuntu

```bash
sudo apt-get install llvm clang cmake
```

#### macOS

1. Install via Homebrew:

```bash
brew install llvm@15 clang cmake
```

2. Add `llvm` to your `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="/opt/homebrew/opt/llvm@15/bin:$PATH"
```

3. Activate the changes:

```bash
source ~/.zshrc
# or
source ~/.bashrc
```

4. Verify that `llvm` is installed:

```bash
llvm-config --version
```

### Build from Source

Ensure the [pre-requisites](#pre-requisites).

And then run:

```sh
$ cargo build
```

> Use `--release` flag for a release build and optimized binary - `./target/release/pulsar`

### Install CLI

#### Using cargo

After ensuring the [pre-requisites](#pre-requisites), just build using cargo:

```sh
$ cargo build --release
```

This would generate an optimized binary.

And then, you can install the binary (optimized) to your system:

```sh
$ cargo install --path .
```

The binary gets added to `~/.cargo/bin`, which is included in the PATH environment variable by default during installation of Rust tools. So you can run it immediately from the shell.

Using this, one doesn't need to download the executable (binary) from the [releases](https://github.com/subspace/pulsar/releases) page each time when there is a new release. They just need to pull the latest code (if already maintained) from the repository and build it locally.
