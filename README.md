# Archive Notice :warning:
As of 2024-03-01, this project has been archived and is no longer actively maintained.

# What does this mean

- **No Updates:** The repository will not be receiving any updates or accepting pull requests. The code is provided as-is.
- **Read-Only:** The repository is now read-only. You can still fork, download, or star the repository.
- **No Support:** We will no longer be responding to issues or questions regarding this project. However, you may still find community support through existing issues or outside forums.

# Why is this project archived?

This project is being archived due to the lack of a sustainable user community and our decision to concentrate our resources on more widely-used projects that are critical to our roadmap towards mainnet.

We believe that focusing our efforts on projects with a broader user base and strategic importance will allow us to make a more significant impact and deliver better value to our community.

# Looking Forward

While this project is being archived, we encourage our vibrant community to take the reins! If you've found value in this project and have ideas for its evolution, we wholeheartedly support and encourage you to fork and develop your own versions. This is an opportunity for innovation and creativity – your contributions could lead to something even more impactful.

For those who are looking for alternatives to this project, we recommend exploring [Space Acres](https://github.com/subspace/space-acres), an opinionated GUI application for farming on [Subspace Network](https://subspace.network/). For those who prefer a CLI experience see the [Advanced CLI](https://docs.subspace.network/docs/farming-&-staking/farming/advanced-cli/cli-install) instructions. 

We extend our deepest gratitude to everyone who has contributed to and supported this project. Your engagement and feedback have been invaluable, and we look forward to seeing how the community takes these ideas forward in new and exciting directions.

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

## Binary

### macOS  

Install using [homebrew](https://brew.sh/) package manager:

```sh
brew tap subspace/homebrew-pulsar
brew install pulsar
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
