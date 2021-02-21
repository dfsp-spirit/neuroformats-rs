use neuroformats::{read_curv};

/// Known meta-data for the "minimal.nii" test file.
#[allow(dead_code)]
pub fn the_demo_curv_file_can_be_read() -> i32 {
    const CURV_FILE: &str = "resources/subjects_dir/subject1/curv/lh.thickness";
    let curv = read_curv(CURV_FILE).unwrap();

    assert_eq!(144848, curv.header.num_vertices);
    assert_eq!(144848, curv.header.num_faces);
    assert_eq!(1, curv.header.num_values_per_vertex);
}