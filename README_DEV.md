# Developer information for neuroformats-rs

## Running the unit tests

Run `cargo test` in the repository root to run the tests locally.


Continuous integration results:

main branch: ![main](https://github.com/dfsp-spirit/neuroformats-rs/actions/workflows/tests.yml/badge.svg?branch=main)

develop branch: ![main](https://github.com/dfsp-spirit/neuroformats-rs/actions/workflows/tests.yml/badge.svg?branch=develop)


## Publishing a new release

* Update the [CHANGES file](./CHANGES)
* Bump version information in [Cargo.toml](./Cargo.toml)
* Run the unit tests: ```cargo test```
* Once everything is ready, publish to crates.io via ```cargo```:

```shell
cargo login
cargo publish --dry-run
```

And when you are satisfied with the result:

```shell
cargo publish
```
