# neuroformats
Handling of structural neuroimaging file formats for [Rust](https://www.rust-lang.org/).

The `neuroformats` crate provides access to structural neuroimaging data in Rust by implementing parsers for various file formats. The focus is on surface-based brain morphometry data, as produced from 3D or 4D magnetic resonance imaging (MRI) data by neuroimaging software suites like [FreeSurfer](http://freesurfer.net/), [CAT12](http://www.neuro.uni-jena.de/cat/) and others.


## Background

In surface-based neuroimaging, basically meshes representing the 3D structure of the human cortex are reconstructed from a segmented 3D brain image. Then properties of the human brain, like the thickness of the cortex at a specific position, are computed from the reconstruction and stored as per-vertex data for the mesh.

![Vis](./resources/img/brainmesh.jpg?raw=true "A mesh representing the white surface of a human brain, with cortical thickness values mapped onto it using the viridis colormap. Different zoom levels are displayed, and the triangles and vertex positions can be identified in the bottom row of images.")

**Fig. 1** *A mesh representing the white surface of a human brain, with cortical thickness values mapped onto it using the viridis colormap. Different magnifications are displayed, and the triangles and vertex positions can be identified in the last one.*


## Installation

The `neuroformats` crate [is on crates.io](https://crates.io/crates/neuroformats).

To use the library in your project, add it as a dependency in your `Cargo.toml` file, e.g.:

```toml
[dependencies]
neuroformats = "0.2.3"
```

## Features

* Read and write FreeSurfer per-vertex data in curv format (like `subject/surf/lh.thickness`): functions `neuroformats::read_curv` and `write_curv`
* Read and write brain meshes in FreeSurfer binary mesh format (like `subject/surf/lh.white`): `read_surf` and `write_surf`
* Read and write FreeSurfer label files (like `subject/label/lh.cortex.label`): `read_label` and `write_label`
* Read FreeSurfer brain surface parcellations (like `subject/label/lh.aparc.annot`): `read_annot`
* Read and write FreeSurfer brain volumes and other data from MGH and MGZ files: `read_mgh` and `write_mgh`

Various utility functions are implemented for performing common computations on the returned structs, e.g. computing the vox2ras matrix from the MGH header data or finding all vertices in a brain surface parcellation that belong to a certain brain atlas region.

## Documentation

### API docs

The `neuroformats` API docs can be found at [docs.rs/neuroformats](https://docs.rs/neuroformats).

### Usage example

Read vertex-wise cortical thickness computed by FreeSurfer:

```rust
use neuroformats::read_curv;
let curv = read_curv("subjects_dir/subject1/surf/lh.thickness").unwrap();
let thickness_at_vertex_0 : f32 = curv.data[0];
```

You now have a `Vec<f32>` with the cortical thickness values in `curv.data`. The order of the values matches the vertex order of the respective brain surface reconstruction (e.g., the white surface mesh of the left brain hemisphere in `subjects_dir/subject1/surf/lh.white`).

See the [neuroformats API docs](https://docs.rs/neuroformats) and the [unit tests in the source code](./src/) for more examples.


## Development

### Unit tests and continuous integration

Continuous integration results:

[![Build Status](https://travis-ci.org/dfsp-spirit/neuroformats-rs.svg?branch=main)](https://travis-ci.org/dfsp-spirit/neuroformats-rs) Travis CI under Linux

### LICENSE

The `neuroformats` crate is free software, dual-licensed under the [MIT](./LICENSE-MIT) or [APACHE-2](./LICENSE-APACHE2) licenses.

### Contributions

Contributions are very welcome. Please get in touch before making major changes to avoid wasted effort.

### Help and contact

If you want to discuss something, need help or found a bug, please [open an issue](https://github.com/dfsp-spirit/neuroformats-rs/issues) here on Github.

The `neuroformats` crate was written by [Tim Schäfer](http://rcmd.org/ts/). You can find my email address on my website if you need to contact me.
