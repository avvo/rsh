# rsh

Rancher SHell.

rsh aims to replicate the features and experience of ssh, but for docker
containers running within Rancher, as such it is a program for connecting into
a remote container and for executing commands on a remote container.

## Usage

    Usage: rsh [opts] [protocol://][user@]host[:port][[/env]/stack]/service [cmd]

    Options:
        -h, --help          Print this message and exit
        -V, --version       Display the version number and exit
        -E LOGFILE          Append debug logs to LOGFILE instead of standard error
        -e CHAR             Sets the escape character (default: `~')
        -F CONFIGFILE       Specifies an alternative configuration file
        -G                  Print the configuration and exit
        -l USER             Specifies the user to log in as on the remote machine
        -o OPTION           Set an option by name
        -p PORT             Port to connect to on the remote host
        -q                  Quiet mode
        -T                  Disable pseudo-terminal allocation
        -t                  Force pseudo-terminal allocation
        -v                  Verbose mode, multiples increase the verbosity

See the [man page][rsh] (and [config man page][rsh_config]) for more details.

[rsh]: rsh.1.ronn
[rsh_config]: rsh_config.5.ronn

## Install

rsh can be installed on macOS via Homebrew[brew]

    brew tap avvo/avvo
    brew install rsh

You can also get the latest release from the [Github releases page][releases]

[brew]: https://brew.sh
[releases]: https://github.com/avvo/rsh/releases

## Developing

rsh is written in Rust, you can install Rust with:

    curl https://sh.rustup.rs -sSf | sh

You can build and run rsh in debug mode with:

    cargo run -- <rsh args>

And build for release with:

    cargo build --release
