// To run this from a terminal:
//     cd neuroformats/examples/brain_rpt
//     cargo run --release
//
// This is based on the dragon example from the `rpt` crate by Eric Zhang.

use rpt::*;
use std::fs;
use color_eyre;
use tempfile::Builder;


// Load a FreeSurfer brain mesh with neuroformats, export as OBJ and re-import with rpt loader.
fn load_brain_from_surf(path: &String) -> color_eyre::Result<Mesh> {
    let surf = neuroformats::read_surf(path)?;
    
    // Export to OBJ format
    let obj_repr: String = surf.mesh.to_obj();
    let dir = Builder::new().prefix("my-temporary-dir").rand_bytes(5).tempdir()?;
    let file_path = dir.path().join("tmp_surf_as.obj");
    fs::write(file_path.clone(), obj_repr).expect("Unable to write tmp OBJ file");

    // Load OBJ
    let obj_file = fs::File::open(file_path)?;
    load_obj(obj_file).map_err(|e| e.into())
}


fn main() {

    println!("=== Neuroformats Example: Raytracing a brain surface with rpt ===");
    println!("* Loading data.");

    let mut scene = Scene::new();

    let brain_lh = load_brain_from_surf(&String::from("../../resources/subjects_dir/subject1/surf/lh.white")).unwrap();
    let brain_rh = load_brain_from_surf(&String::from("../../resources/subjects_dir/subject1/surf/rh.white")).unwrap();
    let output_img = "output.png";

    println!("* Data loaded, creating scene and raytracing. This may take a while...");

    let brain_scale = glm::vec3(0.03, 0.03, 0.03);
    let brain_mat = Material::specular(hex_color(0xBABABA), 0.1);
    
    scene.add(
        Object::new(
            brain_lh
                .scale(&brain_scale)
                //.rotate_y(std::f64::consts::FRAC_PI_2),
        )
        .material(brain_mat),
    );
    scene.add(
        Object::new(
            brain_rh
                .scale(&brain_scale)
                //.rotate_y(std::f64::consts::FRAC_PI_2),
        )
        .material(brain_mat),
    );

    scene.add(Light::Object(
        Object::new(
            sphere()
                .scale(&glm::vec3(2.0, 2.0, 2.0))
                .translate(&glm::vec3(0.0, 12.0, 0.0)),
        )
        .material(Material::light(hex_color(0xFFFFFF), 40.0)),
    ));

    scene.add(Light::Object(
        Object::new(
            sphere()
                .scale(&glm::vec3(2.0, 2.0, 2.0))
                .translate(&glm::vec3(0.0, -3.0, 0.0)),
        )
        .material(Material::light(hex_color(0xFFFFFF), 40.0)),
    ));

    let camera = Camera::look_at(
        glm::vec3(-2.5, 4.0, 6.5), // camera position
        glm::vec3(0.0, -1.5, 0.0), // target // glm::vec3(0.0, -0.25, 0.0),
        glm::vec3(0.0, 1.0, 0.0),  // elevation
        std::f64::consts::FRAC_PI_4, // fov
    );

    Renderer::new(&scene, camera)
        .width(800)
        .height(600)
        .max_bounces(5)
        .num_samples(100)
        .render()
        .save(output_img)
        .unwrap();

    println!("* Done, see output image '{}'.", output_img);
}

