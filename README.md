# neuroformats-rs
Handling of structural neuroimaging file formats for [Rust](https://www.rust-lang.org/).

This crate provides access to structural neuroimaging data in Rust by implementing parsers for various file formats. The focus is on surface-based brain morphometry data, as produced from 3D MRI images by tools like [FreeSurfer](http://freesurfer.net/), [CAT12](http://www.neuro.uni-jena.de/cat/) or others.

## Installation

This is work in progress, come back another day. It will be available on [crates.io](https://crates.io) once a first version is ready.

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

[![codecov](https://codecov.io/gh/dfsp-spirit/neuroformats-rs/branch/main/graph/badge.svg?token=VESCG8GQ9K)](https://codecov.io/gh/dfsp-spirit/neuroformats-rs)
