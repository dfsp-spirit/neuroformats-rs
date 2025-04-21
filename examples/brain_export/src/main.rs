///
/// brain_export -- neuroformats.rs example application that exports a brain mesh with vertex colors based on the sulcal depth.
///
/// This file is part of neuroformats.rs, see https://github.com/dfsp-spirit/neuroformats-rs
///
/// To run this application, run 'cargo run --release' in the examples/brain_export directory.

fn main() {
    println!("=====[ brain_export -- neuroformats.rs mesh export example application ]=====");
    let lh_surf_file = "../../resources/subjects_dir/subject1/surf/lh.white";
    let rh_surf_file = "../../resources/subjects_dir/subject1/surf/rh.white";
    let lh_sulc_file = "../../resources/subjects_dir/subject1/surf/lh.sulc";
    let rh_sulc_file = "../../resources/subjects_dir/subject1/surf/rh.sulc";

    println!("Reading meshes...");
    let lh_surf = neuroformats::read_surf(lh_surf_file).unwrap();
    let rh_surf = neuroformats::read_surf(rh_surf_file).unwrap();

    println!(
        "Computing vertex colors using viridis colormap based on sulcal depth per-vertex values..."
    );
    let lh_colors = lh_surf.colors_from_curv_file(lh_sulc_file).unwrap();
    let rh_colors = rh_surf.colors_from_curv_file(rh_sulc_file).unwrap();

    println!("Constructing and centering merged mesh and colors from both hemisperes......");
    let mut brain = lh_surf.mesh.merge(&rh_surf.mesh);
    brain.move_to(brain.center().unwrap()); // center the mesh at origin

    // merge the colors
    let brain_colors = lh_colors
        .iter()
        .chain(rh_colors.iter())
        .copied()
        .collect::<Vec<_>>();

    // get path of current directory as &path::Path
    let current_dir = std::env::current_dir().unwrap();

    const EXPORT_FILE: &str = "brainmesh_sulc.ply";
    let export_path = current_dir.join(EXPORT_FILE);
    let export_path = export_path.to_str().unwrap();

    let ply_repr = brain.to_ply(Some(&brain_colors));
    std::fs::write(export_path, ply_repr).expect("Unable to write vertex-colored PLY mesh file");

    // Print export file path
    println!("Exported vertex-colored PLY mesh to: {}", export_path);
    println!("Note: You can view the mesh with a mesh viewer software like Blender or MeshLab. If you have MeshLab installed, just run: `meshlab {}`", EXPORT_FILE);
}
