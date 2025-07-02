# decruft

`decruft` is a command-line utility written in Rust designed to help developers identify and manage large or old "cruft"
directories, such as those left behind by your Rust and Node side projects on your filesystem.

It provides a terminal user interface (TUI) for interactive exploration (and destruction) of directories.

This project was initially vibe-coded using Claude Code, but has since been (re)written with human hands.

## Installation

To build and install `decruft`, you'll need to have Rust and Cargo installed.

Once Rust is set up, you can clone this repository and build the project with `cargo run --release`.

## Usage

### Interactive TUI Mode (Default)

To launch `decruft` with the interactive terminal user interface, simply run:

```bash
decruft
```

By default, it will scan the current directory up to a depth of 3. You can customize this:

* Scan a specific directory:
  ```bash
  decruft -d /path/to/your/directory
  ```

* Set a maximum scan depth:
  ```bash
  decruft -m 5 # Scans up to 5 levels deep
  ```

* Combine options:
  ```bash
  decruft -d /var/log -m 2
  ```

### Scan-Only Mode

If you just want to quickly scan and print the results to the console without the TUI, use the `--scan-only` flag:

```bash
decruft --scan-only
```

This mode also supports `-d` and `-m` flags:

```bash
decruft --scan-only -d /home/user/downloads -m 1
```

## Contributing

Contributions are welcome! Please feel free to open issues or submit pull requests.

## License

This project is licensed under the MIT License. See the `LICENSE` file for details.