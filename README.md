# neuroformats-rs
Handling of structural neuroimaging file formats for [Rust](https://www.rust-lang.org/).

This is work in progress, come back another day.

This crate provides access to structural neuroimaging data in Rust by implementing parsers for various file formats. The focus is on surface-based brain morphometry data, as produced from 3D MRI images by tools like [FreeSurfer](http://freesurfer.net/), [CAT12](http://www.neuro.uni-jena.de/cat/) or others.

## Usage example

```rust
use neuroformats::read_curv;
curv = read_curv("path/to/lh.thickness")
```

## Development

### Unit tests and continuous integration

Continuous integration results:

[![Build Status](https://travis-ci.org/dfsp-spirit/neuroformats-rs.svg?branch=main)](https://travis-ci.org/dfsp-spirit/neuroformats-rs) Travis CI under Linux

[![codecov](https://codecov.io/gh/dfsp-spirit/neuroformats-rs/branch/main/graph/badge.svg?token=VESCG8GQ9K)](https://codecov.io/gh/dfsp-spirit/neuroformats-rs)
