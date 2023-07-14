use criterion::{black_box, criterion_group, criterion_main, Criterion};
use neuroformats::{
    read_annot, read_curv, read_label, read_mgh, read_surf, FsAnnot, FsCurv, FsLabel, FsMgh,
    FsSurface,
};

fn fs_annot(file: &str) -> FsAnnot {
    read_annot(file).unwrap()
}

fn fs_curv(file: &str) -> FsCurv {
    read_curv(file).unwrap()
}

fn fs_label(file: &str) -> FsLabel {
    read_label(file).unwrap()
}

fn fs_mgh(file: &str) -> FsMgh {
    read_mgh(file).unwrap()
}

fn fs_surf(file: &str) -> FsSurface {
    read_surf(file).unwrap()
}

fn bench_read(c: &mut Criterion) {
    c.bench_function("fs_annot", |b| {
        b.iter(|| {
            fs_annot(black_box(
                "resources/subjects_dir/subject1/label/lh.aparc.annot",
            ))
        })
    });
    c.bench_function("fs_curv", |b| {
        b.iter(|| {
            fs_curv(black_box(
                "resources/subjects_dir/subject1/surf/lh.thickness",
            ))
        })
    });
    c.bench_function("fs_label", |b| {
        b.iter(|| {
            fs_label(black_box(
                "resources/subjects_dir/subject1/label/lh.entorhinal_exvivo.label",
            ))
        })
    });
    c.bench_function("fs_mgh", |b| {
        b.iter(|| fs_mgh(black_box("resources/subjects_dir/subject1/mri/brain.mgz")))
    });
    c.bench_function("fs_surf", |b| {
        b.iter(|| fs_surf(black_box("resources/subjects_dir/subject1/surf/lh.white")))
    });
}

criterion_group!(benches, bench_read);
criterion_main!(benches);
