use std::path::{Path, PathBuf};

fn main() {
    let lh_surf_file = "../../resources/subjects_dir/subject1/surf/lh.white";
    let rh_surf_file = "../../resources/subjects_dir/subject1/surf/rh.white";

    let lh_surf = neuroformats::read_surf(lh_surf_file).unwrap();
    let rh_surf = neuroformats::read_surf(rh_surf_file).unwrap();

    let lh_sulc =
        neuroformats::read_curv("../../resources/subjects_dir/subject1/surf/lh.sulc").unwrap();
    let rh_sulc =
        neuroformats::read_curv("../../resources/subjects_dir/subject1/surf/rh.sulc").unwrap();

    // Read desikan atlas for the subject
    let lh_annot =
        neuroformats::read_annot("../../resources/subjects_dir/subject1/label/lh.aparc.annot")
            .unwrap();
    let regions: Vec<String> = lh_annot.regions();

    // Print a list of the regions in the left hemisphere
    println!("Regions in the left hemisphere:");
    for region in &regions {
        println!(" * brain region: {}", region);
    }

    // Compute the mean sulcal depth for the bankssts region
    let region_verts_bankssts: Vec<usize> = lh_annot.region_vertices(String::from("bankssts"));
    // regions_verts_bankssts contains the indices of the vertices in the banksts region. Their index is identical to the index in the lh_sulc array.
    let bankssts_sulc: Vec<f32> = region_verts_bankssts
        .iter()
        .map(|&i| lh_sulc.data[i])
        .collect();
    let bankssts_mean_sulc: f32 = bankssts_sulc.iter().sum::<f32>() / bankssts_sulc.len() as f32;
    println!(
        "Mean sulcal depth for bankssts region: {}",
        bankssts_mean_sulc
    );

    let lh_colors = lh_surf
        .colors_from_curv_file("../../resources/subjects_dir/subject1/surf/lh.sulc")
        .unwrap();
    let rh_colors = rh_surf
        .colors_from_curv_file("../../resources/subjects_dir/subject1/surf/rh.sulc")
        .unwrap();

    let brain = lh_surf.mesh.merge(&rh_surf.mesh);

    // merge the colors
    let brain_colors = lh_colors
        .iter()
        .chain(rh_colors.iter())
        .copied()
        .collect::<Vec<_>>();

    // get path of current directory as &path::Path
    let current_dir = std::env::current_dir().unwrap();

    const LH_EXPORT_FILE: &str = "brainmesh.ply";
    let export_path = current_dir.join(LH_EXPORT_FILE);
    let export_path = export_path.to_str().unwrap();

    let ply_repr = brain.to_ply(Some(&brain_colors));
    std::fs::write(export_path, ply_repr).expect("Unable to write vertex-colored PLY mesh file");

    // Print export file path
    println!("Exported vertex-colored PLY mesh to: {}", export_path);
    println!("Note: You can view the mesh with a mesh viewer software like Blender or MeshLab. If you have MeshLab installed, just run: `meshlab {}`", LH_EXPORT_FILE);
}
