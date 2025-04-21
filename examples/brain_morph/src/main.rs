///
/// brain_morph -- neuroformats.rs example application that demonstrates working with native space morphometry data
///
/// This file is part of neuroformats.rs, see https://github.com/dfsp-spirit/neuroformats-rs
///
/// To run this application, run 'cargo run --release' in the examples/brain_morph directory.
///

fn main() {
    println!("=====[ brain_morph -- neuroformats.rs native space morphometry data example application ]=====");
    let lh_surf_file = "../../resources/subjects_dir/subject1/surf/lh.white";
    let rh_surf_file = "../../resources/subjects_dir/subject1/surf/rh.white";
    let lh_thickness_file = "../../resources/subjects_dir/subject1/surf/lh.thickness";
    let rh_thickness_file = "../../resources/subjects_dir/subject1/surf/rh.thickness";
    let lh_cortex_mask_file = "../../resources/subjects_dir/subject1/label/lh.cortex.label";
    let rh_cortex_mask_file = "../../resources/subjects_dir/subject1/label/rh.cortex.label";

    println!("Loading brain surfaces...");
    let lh_surf = neuroformats::read_surf(lh_surf_file).unwrap();
    let rh_surf = neuroformats::read_surf(rh_surf_file).unwrap();

    println!("Loading native space cortical thickness data...");
    let lh_thickness = neuroformats::read_curv(lh_thickness_file).unwrap();
    let rh_thickness = neuroformats::read_curv(rh_thickness_file).unwrap();
    print!("Loaded {} thickness values for left hemisphere, and {} thickness values for right hemisphere.\n",
        lh_thickness.data.len(), rh_thickness.data.len());

    println!("Loading cortical masks to be able to ignore medial wall vertices...");
    let lh_cortex = neuroformats::read_label(lh_cortex_mask_file).unwrap();
    let rh_cortex = neuroformats::read_label(rh_cortex_mask_file).unwrap();

    // Print some information on the mesh and the cortex label
    let lh_cortex_num_verts = lh_cortex.vertexes.len();
    print!(
        "The left surface has {} vertices, of which {} are part of the cortex.\n",
        lh_surf.mesh.num_vertices(),
        lh_cortex_num_verts
    );
    let rh_cortex_num_verts = rh_cortex.vertexes.len();
    print!(
        "The right surface has {} vertices, of which {} are part of the cortex.\n",
        rh_surf.mesh.num_vertices(),
        rh_cortex_num_verts
    );

    // Compute the mean cortical thickness for the left hemisphere, ignoring medial wall (non-cortex) vertices
    let mut lh_cortex_thickness_sum = 0.0;
    for vertex in lh_cortex.vertexes.iter() {
        lh_cortex_thickness_sum += lh_thickness.data[vertex.index as usize];
    }
    let lh_cortex_thickness_mean = lh_cortex_thickness_sum / lh_cortex_num_verts as f32;
    print!(
        "The mean cortical thickness for the left hemisphere is {:.2} mm.\n",
        lh_cortex_thickness_mean
    );

    // Compute the mean cortical thickness for the right hemisphere, ignoring medial wall (non-cortex) vertices
    let mut rh_cortex_thickness_sum = 0.0;
    for vertex in rh_cortex.vertexes.iter() {
        rh_cortex_thickness_sum += rh_thickness.data[vertex.index as usize];
    }
    let rh_cortex_thickness_mean = rh_cortex_thickness_sum / rh_cortex_num_verts as f32;
    print!(
        "The mean cortical thickness for the right hemisphere is {:.2} mm.\n",
        rh_cortex_thickness_mean
    );
}
