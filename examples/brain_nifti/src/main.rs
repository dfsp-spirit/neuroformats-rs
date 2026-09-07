//! A small demo application that shows how to convert brain volumes between the
//! FreeSurfer MGH/MGZ format and the NIfTI-1 format (.nii), and how to access the
//! voxel-to-RAS (vox2ras) information of a volume.
//!
//! It loads the demo brain volume `brain.mgz`, converts it to NIfTI-1, writes it
//! to `brain.nii`, reads it back, and verifies that the two representations of
//! the volume are equivalent (same voxel data and same geometry).
//!
//! Run it from this directory with:
//! ```shell
//! cargo run
//! ```

use neuroformats::{read_mgh, read_nifti, write_nifti, Nifti1};

fn main() {
    // The output NIfTI file is written to the current working directory.
    let out_nii = "brain.nii";

    // 1. Read a FreeSurfer volume (MGH/MGZ).
    let mgh =
        read_mgh("../../resources/subjects_dir/subject1/mri/brain.mgz").expect("reading brain.mgz");

    print!("Loaded MGH volume with dims {:?}, data type {}.", mgh.dim(), mgh.header.dtype);

    // Print the vox2ras matrix (voxel to world coordinates) of the volume, if available.
    match mgh.header.vox2ras() {
        Ok(vox2ras) => {
            println!(" The vox2ras matrix is:");
            for row in vox2ras.rows() {
                println!("    {:>10.4} {:>10.4} {:>10.4} {:>10.4}", row[0], row[1], row[2], row[3]);
            }
        }
        Err(_) => println!(" (the volume carries no RAS information)."),
    }

    // 2. Convert the volume to a NIfTI-1 volume and write it out.
    let nifti = Nifti1::from_mgh(mgh.clone()).expect("volume cannot be written as NIfTI-1");
    write_nifti(out_nii, &nifti).expect("writing brain.nii");
    println!(
        "Wrote NIfTI-1 file '{}' (dim {}, {}, {}, {}, data type {}, s-form code {}, q-form code {}).",
        out_nii,
        nifti.header.dim[1],
        nifti.header.dim[2],
        nifti.header.dim[3],
        nifti.header.dim[4],
        nifti.header.datatype,
        nifti.header.sform_code,
        nifti.header.qform_code
    );

    // 3. Read the NIfTI file back and check that we get the same volume.
    let nifti2 = read_nifti(out_nii).expect("reading brain.nii");
    let mgh2 = nifti2.to_mgh();
    let data1 = mgh.data.mri_uchar.as_ref().expect("uchar data in mgh");
    let data2 = mgh2.data.mri_uchar.as_ref().expect("uchar data in nifti");

    let voxel = [99, 99, 99, 0];
    println!(
        "Voxel value at {:?} is {} in the MGH and {} in the NIfTI file.",
        voxel, data1[voxel], data2[voxel]
    );
    let sum1: u128 = data1.iter().map(|&v| v as u128).sum();
    let sum2: u128 = data2.iter().map(|&v| v as u128).sum();
    assert_eq!(sum1, sum2, "voxel data differs between MGH and NIfTI");
    println!("Voxel sums match ({}), the two files contain the same volume.", sum1);
}
