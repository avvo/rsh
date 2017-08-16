# rsh

Rancher SHell.

## Useage

    rsh [options] [scheme://][user@]host[:port]/[environment/]stack[/service] [command]

## Install

You can get the latest release from the [Github releases page][releases]

[releases]: https://github.com/avvo/rsh/releases

## Developing

rsh is written in Rust, you can install Rust with:

    curl https://sh.rustup.rs -sSf | sh

You can build and run rsh in debug mode with:

    cargo run -- <rsh args>

And build for release with:

    cargo build --release
