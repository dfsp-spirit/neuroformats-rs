fn main() {
    const SURF_FILE: &str = "../../resources/subjects_dir/subject1/surf/lh.white";
    let surf = neuroformats::read_surf(SURF_FILE).unwrap();

    let colors: Vec<u8> = surf
        .colors_from_curv_file("../../resources/subjects_dir/subject1/surf/lh.sulc")
        .unwrap();

    // get path of current directory as &path::Path
    let current_dir = std::env::current_dir().unwrap();

    const LH_EXPORT_FILE: &str = "lh_mesh_sulc_viridis.ply";
    let export_path = current_dir.join(LH_EXPORT_FILE);
    let export_path = export_path.to_str().unwrap();

    let ply_repr = surf.mesh.to_ply(Some(&colors));
    std::fs::write(export_path, ply_repr).expect("Unable to write vertex-colored PLY mesh file");

    // Print export file path
    println!("Exported vertex-colored PLY mesh to: {}", export_path);
    println!("Note: You can view the mesh with a mesh viewer software like Blender or MeshLab. If you have MeshLab installed, just run: `meshlab {}`", LH_EXPORT_FILE);
}
