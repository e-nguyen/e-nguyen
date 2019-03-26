# E-Nguyen

![Prototype Phase Complete!  Real-time-ish Spectrogram](https://raw.githubusercontent.com/e-nguyen/e-nguyen/master/logo/preview_0.1.1.png)

[![CircleCI](https://circleci.com/gh/e-nguyen/e-nguyen.svg?style=shield&circle)](https://circleci.com/gh/e-nguyen/e-nguyen)
[![Published on Crates.io](https://img.shields.io/crates/v/e-nguyen.svg)](https://crates.io/crates/e-nguyen)
[![LGPL Licensed](https://img.shields.io/crates/l/e-nguyen.svg)](https://www.gnu.org/licenses/lgpl-3.0-standalone.html)
[![Donate Ethereum (Taxable)](https://img.shields.io/badge/eth-f98e5f32288750cbfcf08fe5ba21319b400447a4-blueviolet.svg)](https://etherscan.io/address/0xf98e5f32288750cbfcf08fe5ba21319b400447a4)

Produce engaging visual output from arbitrary input, especially sound, in a context designed to composably remix content with minimal prep work.

E-Nguyen is written in Rust and uses the Vulkan graphics API.  The current license is LGPL3+ and content is recommended to submitted as Creative Commons Attribution Share-Alike 4.0+ ([CC-BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/))

The project has two goals:

1.  Seemlessly compose creative works and processing algorithms & heuristics to maximize the richness added by each contribution.
2.  Provide a context for studying and overcoming computing challenges that is highly relaxed and free-form without sacrificing any headroom to skate the bleeding edge.

## Building

E-Nguyen can be built on Rust's stable toolchain.  [Install Rust](https://www.rust-lang.org/tools/install)

You may need to install a Vulkan ICD (Installable Client Driver) for your OS & graphics card combination.  Consult your OS's Vulkan installation documentation.

Clone this repository and run it (building will be performed if necessary).

```shell
git clone https://github.com/e-nguyen/e-nguyen.git
cd e-nguyen
cargo run --release --fullscreen
```

## Troubleshooting

For graphics issues, first try building and running examples from the [Vulkano](https://github.com/vulkano-rs/vulkano) project.  The teapot and other examples should run.

## Contributing

Please review the [Contributing](CONTRIBUTING.md) document and remember to double-emoji your PR's to leave a verifiable record and enable the LGPL and other protections.

## Status

Fresh off the press. Architecture for visualizaiton composition and other documentation inbound.  Get involved!

Currently only the PulseAudio sound server (Linux) can be monitored.
