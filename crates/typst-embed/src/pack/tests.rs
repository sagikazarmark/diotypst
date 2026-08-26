use super::*;
use crate::RenderDate;

/// Decode `.typk` bytes with the raw typst-pack reader, so the assertions
/// below check the archive itself rather than our own round-trip.
fn decode_pack(bytes: &[u8]) -> typst_pack::Pack {
    typst_pack::pack_archive::decode(
        &typst_pack::PackArchiveBytes::from_vec(bytes.to_vec()),
        typst_pack::pack_archive::DecodeLimits::reference_v1(),
    )
    .expect("raw pack should parse")
}

fn sample_pack() -> ProjectPack {
    let project = Project::builder("main.typ")
        .source_file(
            "main.typ",
            "#import \"@demo/badge:0.1.0\": badge\n#include \"chapters/intro.typ\"",
        )
        .source_file("chapters/intro.typ", "= Intro")
        .file("assets/logo.png", b"\x89PNG".to_vec())
        .build()
        .expect("sample project should be valid");
    let bundle = PackageBundle::builder(
        "@demo/badge:0.1.0"
            .parse()
            .expect("sample spec should parse"),
    )
    .file("typst.toml", b"[package]".to_vec())
    .file("lib.typ", b"#let badge(body) = body".to_vec())
    .build()
    .expect("sample bundle should be valid");

    ProjectPack::builder(project)
        .package_bundle(bundle)
        .external_package_bundle(
            PackageBundle::builder(
                "@preview/cetz:0.4.2"
                    .parse()
                    .expect("external spec should parse"),
            )
            .file("typst.toml", b"[package]".to_vec())
            .file("lib.typ", b"".to_vec())
            .build()
            .expect("external bundle should be valid"),
        )
        .metadata(
            ProjectPackMetadata::new()
                .with_name("Sample")
                .with_author("Demo"),
        )
        .build()
        .expect("sample pack should build")
}

#[test]
fn project_pack_round_trips_through_typk_bytes() {
    let pack = sample_pack();

    let bytes = pack.to_bytes().expect("pack should serialize");
    let raw = decode_pack(&bytes);
    let external = raw
        .package_requirements()
        .iter()
        .find(|requirement| !requirement.is_embedded())
        .expect("external package requirement should be recorded");
    assert_eq!(external.spec().to_string(), "@preview/cetz:0.4.2");
    assert_eq!(external.file_count(), 2);
    assert!(!raw.has_package(external.spec()));

    let read = ProjectPack::from_bytes(&bytes).expect("pack should parse back");

    assert_eq!(read.project().root_path().get_without_slash(), "main.typ");
    assert_eq!(
        read.project().file_bytes("chapters/intro.typ"),
        Some(b"= Intro".as_slice())
    );
    assert_eq!(read.package_bundles().len(), 1);
    assert_eq!(
        read.package_bundles()[0].file_bytes("lib.typ"),
        Some(b"#let badge(body) = body".as_slice())
    );
    assert_eq!(
        read.external_packages(),
        &["@preview/cetz:0.4.2"
            .parse::<PackageSpec>()
            .expect("external spec should parse")]
    );
    let metadata = read.metadata().expect("metadata should survive");
    assert_eq!(metadata.name(), Some("Sample"));
    assert_eq!(metadata.authors(), ["Demo".to_owned()]);
    assert_eq!(
        read.to_bytes().expect("loaded pack should serialize"),
        bytes
    );
}

#[test]
fn project_pack_embeds_and_restores_font_files() {
    let font = typst_assets::fonts()
        .next()
        .expect("bundled fonts should not be empty")
        .to_vec();
    let pack = ProjectPack::builder(Project::from_source("Hello"))
        .font_file(font.clone())
        .build()
        .expect("pack with a font should build");

    let bytes = pack.to_bytes().expect("pack should serialize");
    let read = ProjectPack::from_bytes(&bytes).expect("pack should parse back");

    assert_eq!(read.font_files(), std::slice::from_ref(&font));
    assert_eq!(read.font_set().container_files_where(|_| true), [font]);
}

#[test]
fn project_pack_render_environment_installs_vendored_bundles() {
    let environment = sample_pack()
        .preparation_environment()
        .expect("environment should build");

    let bundle = environment
        .package_bundle(&"@demo/badge:0.1.0".parse().expect("spec should parse"))
        .expect("vendored bundle should be installed");
    assert_eq!(
        bundle.file_bytes("typst.toml"),
        Some(b"[package]".as_slice())
    );
}

#[test]
fn project_pack_verifies_exact_external_package_trees() {
    let pack = sample_pack();
    let matching = PackageBundle::builder(
        "@preview/cetz:0.4.2"
            .parse()
            .expect("external spec should parse"),
    )
    .file("typst.toml", b"[package]".to_vec())
    .file("lib.typ", b"".to_vec())
    .build()
    .expect("matching bundle should build");
    assert!(matches!(
        pack.render_environment(),
        Err(ProjectPackError::MissingExternalPackage { .. })
    ));

    let matching_environment = pack
        .preparation_environment()
        .expect("base environment should build")
        .to_builder()
        .package_bundle(matching)
        .build()
        .expect("matching environment should build");
    pack.verify_external_packages(&matching_environment)
        .expect("matching package tree should verify");
    pack.render_environment_with_external_packages([matching_environment
        .package_bundle(
            &"@preview/cetz:0.4.2"
                .parse()
                .expect("external spec should parse"),
        )
        .expect("matching bundle should be installed")
        .clone()])
        .expect("verified render environment should build");

    let unexpected_spec: PackageSpec = "@demo/unexpected:0.1.0"
        .parse()
        .expect("unexpected spec should parse");
    let unexpected = PackageBundle::builder(unexpected_spec.clone())
        .file("lib.typ", b"".to_vec())
        .build()
        .expect("unexpected bundle should build");
    let render_date = RenderDate::from_ymd(2030, 2, 3).expect("date should be valid");
    let base = matching_environment
        .to_builder()
        .package_bundle(unexpected.clone())
        .render_date(render_date)
        .input("tenant", "demo")
        .build()
        .expect("base environment should build");
    let rendered = pack
        .render_environment_from(&base)
        .expect("base should fulfill the pack");
    assert_eq!(rendered.render_date(), render_date);
    assert_eq!(rendered.inputs(), base.inputs());
    assert!(rendered.package_bundle(&unexpected_spec).is_none());

    assert!(matches!(
        pack.render_environment_with_external_packages([unexpected]),
        Err(ProjectPackError::UnexpectedExternalPackage { .. })
    ));

    let mismatched = PackageBundle::builder(
        "@preview/cetz:0.4.2"
            .parse()
            .expect("external spec should parse"),
    )
    .file("typst.toml", b"[package]".to_vec())
    .file("lib.typ", b"changed".to_vec())
    .build()
    .expect("mismatched bundle should build");
    let mismatched_environment = pack
        .preparation_environment()
        .expect("base environment should build")
        .to_builder()
        .package_bundle(mismatched)
        .build()
        .expect("mismatched environment should build");
    assert_eq!(
        pack.verify_external_packages(&mismatched_environment),
        Err(ProjectPackError::MismatchedExternalPackage {
            spec: "@preview/cetz:0.4.2"
                .parse()
                .expect("external spec should parse"),
        })
    );
}

#[test]
fn project_pack_rejects_garbage_bytes() {
    let result = ProjectPack::from_bytes(b"not a pack");

    assert!(matches!(result, Err(ProjectPackError::Archive { .. })));
}

#[test]
fn project_pack_fulfills_external_fonts_from_a_base_environment() {
    let external_font = typst_assets::fonts()
        .next()
        .expect("bundled fonts should not be empty")
        .to_vec();
    let embedded_font = typst_assets::fonts()
        .nth(1)
        .expect("bundled fonts should contain a second font")
        .to_vec();
    let ambient_font = typst_assets::fonts()
        .nth(2)
        .expect("bundled fonts should contain a third font")
        .to_vec();
    let pack = ProjectPack::builder(Project::from_source("Hello"))
        .external_font_face(external_font.clone(), 0)
        .font_face(embedded_font.clone(), 0)
        .build()
        .expect("pack with an external font should build");

    assert_eq!(pack.external_font_requirements().len(), 1);
    let raw = decode_pack(&pack.to_bytes().expect("pack should serialize"));
    assert!(!raw.font_catalog()[0].is_embedded());
    assert!(raw.font_catalog()[1].is_embedded());
    assert!(matches!(
        pack.render_environment(),
        Err(ProjectPackError::MissingExternalFont { .. })
    ));

    let base = RenderEnvironment::builder()
        .font_set(FontSet::from_font_files([
            ambient_font,
            external_font.clone(),
        ]))
        .build()
        .expect("base environment should build");
    let environment = pack
        .render_environment_from(&base)
        .expect("base font should fulfill the pack");

    let store = environment.font_set().font_store();
    assert_eq!(
        store.font(0).expect("external face").data().as_slice(),
        external_font
    );
    assert_eq!(
        store.font(1).expect("embedded face").data().as_slice(),
        embedded_font
    );
    assert!(store.font(2).is_none());
}

#[test]
fn project_pack_builder_rejects_unrecognized_fonts_and_duplicate_packages() {
    let font_result = ProjectPack::builder(Project::from_source("Hello"))
        .font_file(b"not a font".to_vec())
        .build();
    assert_eq!(font_result, Err(ProjectPackError::UnrecognizedFont));

    let empty_external = PackageBundle::builder(
        "@demo/empty:0.1.0"
            .parse()
            .expect("external spec should parse"),
    )
    .build()
    .expect("empty package bundle should build");
    assert_eq!(
        ProjectPack::builder(Project::from_source("Hello"))
            .external_package_bundle(empty_external)
            .build(),
        Err(ProjectPackError::EmptyExternalPackage {
            spec: "@demo/empty:0.1.0"
                .parse()
                .expect("external spec should parse"),
        })
    );

    let bundle = |spec: &str| {
        PackageBundle::builder(spec.parse().expect("spec should parse"))
            .file("lib.typ", b"".to_vec())
            .build()
            .expect("bundle should build")
    };
    let duplicate_result = ProjectPack::builder(Project::from_source("Hello"))
        .package_bundle(bundle("@demo/badge:0.1.0"))
        .package_bundle(bundle("@demo/badge:0.1.0"))
        .build();
    assert_eq!(
        duplicate_result,
        Err(ProjectPackError::DuplicatePackage {
            spec: "@demo/badge:0.1.0".parse().expect("spec should parse"),
        })
    );
}

#[test]
fn project_pack_reads_archives_written_by_typst_pack_directly() {
    // Interop guard: a pack assembled with the raw typst-pack builder,
    // not just our own writer, converts into crate domain types.
    let pack = typst_pack::pack_archive::encode(
        &typst_pack::Pack::builder("main.typ")
            .file("main.typ", b"Hello".to_vec())
            .expect("file should be valid")
            .build()
            .expect("raw pack should build"),
    )
    .expect("raw pack should serialize")
    .into_vec();

    let read = ProjectPack::from_bytes(&pack).expect("raw pack should parse");

    assert_eq!(read.project().root_path().get_without_slash(), "main.typ");
    assert_eq!(
        read.project().file_bytes("main.typ"),
        Some(b"Hello".as_slice())
    );
}

/// A golden version-1 `.typk` archive, written by typst-pack 0.4.0.
///
/// This pins the decoder against a fixed archive rather than against whatever
/// the current encoder happens to emit, so a format-level regression in any
/// future typst-pack upgrade fails here.
///
/// It is not, by itself, evidence of backward compatibility: 0.5's encoder
/// reproduces these exact bytes, because every construct in the fixture is
/// encoded identically by both releases. The one construct that would differ
/// is an embedded font, which 0.4 stored at `fonts/<family-slug>.<ext>` and
/// 0.5 stores at `fonts/<hex-digest>.<ext>`. Covering that path needs a
/// font-bearing fixture, and the smallest bundled font is ~250 KB.
const LEGACY_PACK: &[u8] = include_bytes!("testdata/typst-pack-0.4.0.typk");

#[test]
fn a_golden_version_1_archive_still_reads() {
    let pack = ProjectPack::from_bytes(LEGACY_PACK).expect("a version-1 archive should read");

    assert_eq!(pack.project().root_path().get_without_slash(), "main.typ");
    assert_eq!(
        pack.project().file_bytes("chapters/intro.typ"),
        Some(b"= Intro".as_slice())
    );
    assert_eq!(
        pack.project().file_bytes("assets/logo.png"),
        Some(b"\x89PNG".as_slice())
    );

    let vendored = pack.package_bundles();
    assert_eq!(vendored.len(), 1);
    assert_eq!(vendored[0].spec().to_string(), "@demo/badge:0.1.0");

    let external = pack.external_package_requirements();
    assert_eq!(external.len(), 1);
    assert_eq!(external[0].spec().to_string(), "@preview/cetz:0.4.2");
    assert_eq!(external[0].file_count(), 2);

    let metadata = pack.metadata().expect("the archive carries metadata");
    assert_eq!(metadata.name(), Some("Legacy fixture"));
    assert_eq!(metadata.authors(), ["diotypst"]);
}

#[test]
fn the_font_container_digest_survived_the_typst_pack_rewrite() {
    // 0.4.0 computed this with `FontContainerIdentity::from_bytes`, which 0.5
    // replaced with `CanonicalIdentity::for_font_container_bytes`. A changed
    // digest would silently invalidate every font requirement in an existing
    // pack, so the value is pinned rather than recomputed.
    const SAMPLE: &[u8] = b"diotypst font container fixture";
    const DIGEST_FROM_0_4_0: [u8; 16] = [
        151, 187, 175, 218, 114, 58, 14, 58, 233, 50, 63, 102, 155, 208, 209, 184,
    ];

    assert_eq!(
        typst_pack::CanonicalIdentity::for_font_container_bytes(SAMPLE).digest(),
        DIGEST_FROM_0_4_0
    );
}
