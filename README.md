# neuroformats-rs
Handling of structural neuroimaging file formats for [Rust](https://www.rust-lang.org/).

This crate provides access to structural neuroimaging data in Rust by implementing parsers for various file formats. The focus is on surface-based brain morphometry data, as produced from 3D MRI images by tools like [FreeSurfer](http://freesurfer.net/), [CAT12](http://www.neuro.uni-jena.de/cat/) or others.

## Installation

A very early version of the `neuroformats` crate is now on [crates.io](https://crates.io/crates/neuroformats).

To use the library in your project, add it as a dependency in your `Cargo.toml` file, e.g.:

```toml
...
[dependencies]
neuroformats = "0.1.0"
```

## Features

* Read FreeSurfer per-vertex data in curv format (like `subject/surf/lh.thickness`): function `neuroformats::read_curv`
* Read brain meshes in FreeSurfer binary mesh format (like `subject/surf/lh.white`): `neuroformats::read_surf`
* Read FreeSurfer label files (like `subject/label/lh.cortex.label`): `neuroformats::read_label`
* Read FreeSurfer brain surface parcellations (like `subject/label/lh.aparc.annot`): `neuroformats::read_annot`


## Usage example

Read vertex-wise cortical thickness computed by FreeSurfer:

```rust
use neuroformats::read_curv;
curv = read_curv("subjects_dir/subject1/surf/lh.thickness");
```

You now have a `Vec<f32>` with the cortical thickness values in `curv.data`. The order of the values matches the vertex order of the respective brain surface reconstruction (e.g., the white surface mesh of the left brain hemisphere in `subjects_dir/subject1/surf/lh.white`).


## Development

### Unit tests and continuous integration

Continuous integration results:

[![Build Status](https://travis-ci.org/dfsp-spirit/neuroformats-rs.svg?branch=main)](https://travis-ci.org/dfsp-spirit/neuroformats-rs) Travis CI under Linux

### LICENSE

The `neuroformats` crate is free software, dual-licensed under the [MIT](./LICENSE-MIT) or [APACHE-2](./LICENSE-APACHE2) licenses.
