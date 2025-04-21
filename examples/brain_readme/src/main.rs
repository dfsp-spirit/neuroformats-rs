fn main() {
    // Read the brain mesh of the left hemisphere
    let lh_surface =
        neuroformats::read_surf("../../resources/subjects_dir/subject1/surf/lh.white").unwrap();

    // Read morphometry data (native space cortical thickness per vertex) for the mesh
    let lh_thickness =
        neuroformats::read_curv("../../resources/subjects_dir/subject1/surf/lh.thickness").unwrap();

    // Load cortical mask
    let lh_cortex =
        neuroformats::read_label("../../resources/subjects_dir/subject1/label/lh.cortex.label")
            .unwrap();

    // Print some info
    print!(
        "The left surface has {} vertices, of which {} are part of the cortex.\n",
        lh_surface.mesh.num_vertices(),
        lh_cortex.vertexes.len()
    );

    print!(
        "The cortical thickness at vertex 0 is {:.2} mm.\n",
        lh_thickness.data[0]
    );

    // Compute the mean cortical thickness for the left hemisphere, ignoring medial wall (non-cortex) vertices
    let mut lh_cortex_thickness_sum = 0.0;
    for vertex in lh_cortex.vertexes.iter() {
        lh_cortex_thickness_sum += lh_thickness.data[vertex.index as usize];
    }
    let lh_cortex_thickness_mean = lh_cortex_thickness_sum / lh_cortex.vertexes.len() as f32;
    print!(
        "The mean cortical thickness for the left hemisphere is {:.2} mm.\n",
        lh_cortex_thickness_mean
    );
}
