// SPDX-License-Identifier: MIT

use super::*;
use std::io::Write;

#[test]
fn large_binary_stl_is_accepted_and_over_limit_reports_triangles() {
    for (count, accepted) in [(1_620_000u32, true), (2_000_001, false)] {
        let mut bytes = vec![0; 84 + count as usize * 50];
        bytes[80..84].copy_from_slice(&count.to_le_bytes());
        let result = stl(&bytes);
        if accepted {
            assert_eq!(result.expect("81 MB STL").len(), count as usize);
        } else {
            assert!(
                result
                    .expect_err("triangle budget")
                    .contains("2 million triangle")
            );
        }
    }
}

#[test]
fn freecad_thumbnail_needs_no_geometry_reader_and_missing_thumbnail_is_unavailable() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let path = directory.path().join("sample.FCStd");
    for include_thumbnail in [true, false] {
        let mut package = zip::ZipWriter::new(fs::File::create(&path).expect("FreeCAD package"));
        let options = zip::write::SimpleFileOptions::default();
        package
            .start_file("Document.xml", options)
            .expect("document part");
        package
            .write_all(b"geometry deliberately not parsed")
            .expect("document");
        if include_thumbnail {
            package
                .start_file("thumbnails/Thumbnail.png", options)
                .expect("thumbnail part");
            let mut thumbnail = Pixmap::new(24, 24).expect("pixmap");
            thumbnail.fill(Color::from_rgba8(255, 0, 0, 255));
            package
                .write_all(&thumbnail.encode_png().expect("PNG"))
                .expect("thumbnail");
        }
        package.finish().expect("finished package");
        let result = render(&path, "200x200:00ff00:101010");
        if include_thumbnail {
            let png = result.expect("FreeCAD thumbnail");
            assert_eq!(
                Pixmap::decode_png(&png)
                    .expect("decoded thumbnail")
                    .pixel(0, 0)
                    .expect("pixel")
                    .red(),
                255
            );
        } else {
            assert!(
                result
                    .expect_err("missing thumbnail")
                    .contains("no embedded thumbnail")
            );
        }
    }
}

#[test]
fn binary_stl_with_solid_header_is_not_misread_as_ascii() {
    let mut bytes = vec![0; 84 + 50];
    bytes[..5].copy_from_slice(b"solid");
    bytes[80..84].copy_from_slice(&1u32.to_le_bytes());
    let points = [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    for (vertex, point) in points.iter().enumerate() {
        for (axis, value) in point.iter().enumerate() {
            let at = 84 + 12 + vertex * 12 + axis * 4;
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    assert_eq!(stl(&bytes).expect("binary STL"), vec![points]);
}

#[test]
fn ascii_stl_parses_faces_and_rejects_non_finite_coordinates() {
    let source = b"solid sample\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid";
    assert_eq!(stl(source).expect("ASCII STL").len(), 1);
    assert!(stl(b"solid sample\nvertex 0 0 0\nvertex NaN 0 0\nvertex 0 1 0\nendsolid").is_err());
}

#[test]
fn package_renders_build_items_and_component_transforms() {
    let model = br#"<model><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object><object id="2"><components><component objectid="1" transform="1 0 0 0 1 0 0 0 1 10 0 0"/></components></object></resources><build><item objectid="2" transform="1 0 0 0 1 0 0 0 1 0 5 0"/></build></model>"#;
    let triangles = triangles_3mf(model).expect("3MF model");
    assert_eq!(
        triangles,
        vec![[[10., 5., 0.], [11., 5., 0.], [10., 6., 0.]]]
    );
}

#[test]
fn cyclic_components_are_bounded() {
    let model = br#"<model><resources><object id="1"><components><component objectid="1"/></components></object></resources><build><item objectid="1"/></build></model>"#;
    assert!(triangles_3mf(model).is_err());
}

#[test]
fn embedded_thumbnail_is_preferred_and_invalid_thumbnail_falls_back_to_theme_render() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let path = directory.path().join("sample.3mf");
    let file = fs::File::create(&path).expect("package");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("3D/3dmodel.model", options)
        .expect("model part");
    zip.write_all(br#"<model><resources><object id="1"><mesh><vertices><vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/></vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object></resources><build><item objectid="1"/></build></model>"#).expect("model content");
    zip.start_file("Metadata/thumbnail.png", options)
        .expect("thumbnail part");
    let mut thumbnail = Pixmap::new(8, 8).expect("thumbnail pixmap");
    thumbnail.fill(Color::from_rgba8(255, 0, 0, 255));
    zip.write_all(&thumbnail.encode_png().expect("thumbnail PNG"))
        .expect("thumbnail content");
    zip.finish().expect("package complete");

    let embedded = render(&path, "200x200:00ff00:101010").expect("embedded thumbnail");
    assert_eq!(
        Pixmap::decode_png(&embedded)
            .expect("thumbnail")
            .pixel(0, 0)
            .expect("pixel")
            .red(),
        255
    );
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("package");
    let mut zip = zip::ZipWriter::new_append(file).expect("append to package");
    zip.start_file("Metadata/broken-thumbnail.png", options)
        .expect("broken thumbnail part");
    zip.write_all(b"not an image").expect("broken thumbnail");
    zip.finish().expect("package complete");
    assert_eq!(
        render(&path, "200x200:00ff00:101010").expect("valid thumbnail still preferred"),
        embedded
    );
    let mut package = zip::ZipArchive::new(fs::File::open(&path).expect("package")).expect("ZIP");
    let mut model = Vec::new();
    package
        .by_name("3D/3dmodel.model")
        .expect("model")
        .read_to_end(&mut model)
        .expect("model bytes");
    drop(package);
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).expect("package"));
    zip.start_file("3D/3dmodel.model", options)
        .expect("model part");
    zip.write_all(&model).expect("model");
    zip.start_file("Metadata/thumbnail.png", options)
        .expect("thumbnail part");
    zip.write_all(b"not an image").expect("invalid thumbnail");
    zip.finish().expect("package complete");
    let shaded = render(&path, "200x200:00ff00:101010").expect("rendered model");
    assert_ne!(embedded, shaded);
    let recolored = render(&path, "200x200:0000ff:101010").expect("recolored model");
    assert_ne!(shaded, recolored);
}
