///
/// brain_atlas -- neuroformats.rs example application that demonstrates working with a brain atlas, like Desikan-Killiany
///
/// This file is part of neuroformats.rs, see https://github.com/dfsp-spirit/neuroformats-rs
///
/// To run this application, run 'cargo run --release' in the examples/brain_atlas directory.

fn main() {
    println!("=====[ brain_atlas -- neuroformats.rs surface atlas example application ]=====");
    let lh_surf_file = "../../resources/subjects_dir/subject1/surf/lh.white";
    let rh_surf_file = "../../resources/subjects_dir/subject1/surf/rh.white";
    let lh_atlas_file = "../../resources/subjects_dir/subject1/label/lh.aparc.annot";
    let rh_atlas_file = "../../resources/subjects_dir/subject1/label/rh.aparc.annot";

    // read the brain surfaces (meshes)
    println!("Loading brain surfaces...");
    let lh_surf = neuroformats::read_surf(lh_surf_file).unwrap();
    let rh_surf = neuroformats::read_surf(rh_surf_file).unwrap();

    println!("Loading surface atlas...");
    // Read desikan surface atlas for the subject
    let lh_annot = neuroformats::read_annot(lh_atlas_file).unwrap();
    let rh_annot = neuroformats::read_annot(rh_atlas_file).unwrap();

    println!("Chekcing atlas regions for left hemisphere...");
    // Extract brain region names from the annotation files
    let lh_regions: Vec<String> = lh_annot.regions();
    let rh_regions: Vec<String> = rh_annot.regions();

    // Print a list of the regions in the left hemisphere. In general, the regions from the right hemi should be identical,
    // though there are rare cases with severe FreeSurfer reconstruction errors (e.g., due to a bad quality MRI scan) where
    // no vertices were assigned to a region in one hemisphere because the segmentation failed.
    // In such a case, that region is not present in the atlas for the respective hemisphere, so one cannot assume that the
    // regions are identical in both hemispheres if you are not sure about the quality of the data.
    // For this subject and scan, we know we are fine though, which we demonstarte here by checking that the regions are identical.

    // Compare the lh_regions and rh_regions, and make sure that the regions are identical.
    if lh_regions.len() != rh_regions.len() {
        panic!("The number of regions in the left and right hemisphere do not match!");
    }

    println!("Regions in the left hemisphere:");
    for region in &lh_regions {
        if !rh_regions.contains(region) {
            panic!(
                "Left hemisphere atlas region {} is not present in the right hemisphere!",
                region
            );
        }
        println!(" * brain region: {}", region);
    }

    // Compute the mean cortical thickness for the bankssts region of the left hemisphere.
    // Note that due to partial volume effects, the values you get in this way may deviate a bit from those
    // reported by FreeSurfer in its stats files.

    // Read the cortical thickness file for the left hemisphere
    println!("Computing mean cortical thickness in bankssts region...");
    let lh_thickness_file = "../../resources/subjects_dir/subject1/surf/lh.thickness";
    let lh_thickness = neuroformats::read_curv(lh_thickness_file).unwrap();

    // Extract the vertices in the bankssts region (the vertex indices, to be precise).
    let region_verts_bankssts: Vec<usize> = lh_annot.region_vertices(String::from("bankssts"));
    let bankssts_thickness_values: Vec<f32> = region_verts_bankssts
        .iter()
        .map(|&i| lh_thickness.data[i])
        .collect();
    let bankssts_mean_thickness: f32 =
        bankssts_thickness_values.iter().sum::<f32>() / bankssts_thickness_values.len() as f32;
    println!(
        "Mean cortical thickness for the 'bankssts' brain region of the left hemisphere consisting of {} vertices: {}",
        region_verts_bankssts.len(), bankssts_mean_thickness
    );

    println!("Preparing brain mesh for export...");
    // Get the vertex colors for the atlas regions, and apply them to the brain meshes.
    let lh_colors = lh_annot.vertex_colors(false, 0);
    let rh_colors = rh_annot.vertex_colors(false, 0);

    let mut brain = lh_surf.mesh.merge(&rh_surf.mesh); // Combine the left and right hemisphere meshes into one
    brain.move_to(brain.center().unwrap()); // center the mesh at origin

    // merge the left and right colors as well
    let brain_colors = lh_colors
        .iter()
        .chain(rh_colors.iter())
        .copied()
        .collect::<Vec<_>>();

    // Construct the export path for the PLY file and write it to disk
    let current_dir = std::env::current_dir().unwrap();
    const EXPORT_FILE: &str = "brainmesh.ply";
    let export_path = current_dir.join(EXPORT_FILE);
    let export_path = export_path.to_str().unwrap();

    let ply_repr = brain.to_ply(Some(&brain_colors));
    std::fs::write(export_path, ply_repr).expect("Unable to write vertex-colored PLY mesh file");

    // Print export file path
    println!("Exported vertex-colored PLY mesh to: {}", export_path);
    println!("Note: You can view the mesh with a mesh viewer software like Blender or MeshLab. If you have MeshLab installed, just run: `meshlab {}`", EXPORT_FILE);
}
