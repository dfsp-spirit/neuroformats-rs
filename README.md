# neuroformats
Handling of structural neuroimaging file formats for [Rust](https://www.rust-lang.org/).

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.8128102.svg)](https://doi.org/10.5281/zenodo.8128102)
[<img alt="crates.io" src="https://img.shields.io/crates/v/neuroformats.svg?logo=rust" height="20">](https://crates.io/crates/neuroformats)
[![docs.rs](https://img.shields.io/docsrs/neuroformats/0.2.3)](https://docs.rs/neuroformats/)
![main](https://github.com/dfsp-spirit/neuroformats-rs/actions/workflows/tests.yml/badge.svg?branch=main)

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
* Read FreeSurfer brain surface parcellations (a.k.a. brain atlas, like `subject/label/lh.aparc.annot`): `read_annot`
* Read and write FreeSurfer brain volumes and other data from MGH and MGZ files: `read_mgh` and `write_mgh`

Various utility functions are implemented for performing common computations on the returned structs, e.g. computing the vox2ras matrix from the MGH header data or finding all vertices in a brain surface parcellation that belong to a certain brain atlas region. The library can also export brain meshes in standard mesh formats (Wavefront Object Format, PLY format, glTF), optionally with vertex colors based on per-vertex data (from curv or MGH/MGZ files) and the viridis colormap for quick visualization in tools like MeshLab or Blender.

## Documentation

### API docs

The `neuroformats` API docs can be found at [docs.rs/neuroformats](https://docs.rs/neuroformats).

### Short usage example

Read vertex-wise cortical thickness computed by FreeSurfer:

```rust
use neuroformats::read_curv;
let curv = read_curv("subjects_dir/subject1/surf/lh.thickness").unwrap();
let thickness_at_vertex_0 : f32 = curv.data[0];
```

You now have a `Vec<f32>` with the cortical thickness values in `curv.data`. The order of the values matches the vertex order of the respective brain surface reconstruction (e.g., the white surface mesh of the left brain hemisphere in `subjects_dir/subject1/surf/lh.white`).

### Full demo applications

* [brain_morph](./examples/brain_morph/src/main.rs): A simple application that demonstrates working with morphometry data. It shows how to load brain surfaces and the cortical thickness values for each vertex. It then proceeds to load a cortex mask, and uses it to compute the average cortical thickness per hemisphere, restricted to cortical vertices (i.e., ignoring the medial wall).
* [brain_atlas](./examples/brain_atlas/src/main.rs): Demonstrates how to load a brain surface atlas (the Desikan-Killiany atlas), find the vertices that belong to a specific atlas region, and the respective morphometry values for these vertices. The app then computes the average cortical thickness in a brain region.
* [brain_export](./examples/brain_export/src/main.rs) This app loads a brain mesh and per-vertex data (sulcal depth at each vertex), and maps the per-vertex values to colors using the viridis colormap. It does this for both hemispheres, then combines the meshes into a single mesh, centers it at the origin, merges the color values as well, and exports the result as a vertex-colored PLY file. The resulting file can be visualized in standard mesh viewers like Blender or MeshLab.

To run the demo apps, type ```cargo run```in the respective directory, e.g., in [./examples/brain_morph/](./examples/brain_morph/).

See the [neuroformats API docs](https://docs.rs/neuroformats) and the [unit tests in the source code](./src/) for more examples for using the neuroformats functions.

#### Related demo apps

These apps do not directly illustrate using the neuroformats API, but use Rust-based tools to visualize the exported brain meshes.

* There is a small command line demo application that loads a brain surface mesh and raytraces an image based on the mesh to a PNG file available in the [./examples/brain_rpt directory](./examples/brain_rpt/src/main.rs). The demo uses the [rpt crate by Eric Zhang and Alexander Morozov](https://lib.rs/crates/rpt) to do the raytracing. This can take quite a while on slower computers (more than 10 minutes on my laptop).
* A simple demo that loads a brain mesh into a simple scene using the bevy game engine for real-time viewing can be found in the [./examples/brain_bevy directory](./examples/brain_bevy/src/main.rs). This one requires some non-Rust system dependencies to run and is thus a bit harder to install. See the instructions in the main file if you're using some Debian-based Linux like Ubuntu.



## Development Info

Please see the [README_DEV file](./README_DEV.md).


### LICENSE

The `neuroformats` crate is free software, dual-licensed under the [MIT](./LICENSE-MIT) or [APACHE-2](./LICENSE-APACHE2) licenses.

### Contributions

Contributions are very welcome. Please get in touch before making major changes to avoid wasted effort.

### Help and contact

If you want to discuss something, need help or found a bug, please [open an issue](https://github.com/dfsp-spirit/neuroformats-rs/issues) here on Github.

The `neuroformats` crate was written by [Tim Schäfer](https://ts.rcmd.org). You can find my email address on my website if you need to contact me.
