use super::*;
use crate::RenderDate;

/// Decode `.typk` bytes with the raw typst-pack reader, so the assertions
/// below check the archive itself rather than our own round-trip.
fn decode_pack(bytes: &[u8]) -> typst_pack::Pack {
    typst_pack::archive::decode(bytes, typst_pack::archive::DecodeLimits::default())
        .expect("raw pack should parse")
}

/// A `typst.toml` declaring `name` and `version`, which typst-pack requires
/// of any package tree that carries one.
fn package_manifest(name: &str, version: &str) -> Vec<u8> {
    format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\nentrypoint = \"lib.typ\"\n")
        .into_bytes()
}

fn cetz_bundle(lib: &[u8]) -> PackageBundle {
    PackageBundle::builder(
        "@preview/cetz:0.4.2"
            .parse()
            .expect("external spec should parse"),
    )
    .file("typst.toml", package_manifest("cetz", "0.4.2"))
    .file("lib.typ", lib.to_vec())
    .build()
    .expect("cetz bundle should be valid")
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
    .file("typst.toml", package_manifest("badge", "0.1.0"))
    .file("lib.typ", b"#let badge(body) = body".to_vec())
    .build()
    .expect("sample bundle should be valid");

    ProjectPack::builder(project)
        .package_bundle(bundle)
        .external_package_bundle(cetz_bundle(b""))
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
        .find(|requirement| !requirement.embedded)
        .expect("external package requirement should be recorded");
    assert_eq!(external.spec.to_string(), "@preview/cetz:0.4.2");
    assert!(raw.packages().all(|(spec, _)| *spec != external.spec));

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
    assert_eq!(
        read.external_package_requirements()[0].tree_digest(),
        *external.digest.as_bytes()
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
        Some(package_manifest("badge", "0.1.0").as_slice())
    );
}

#[test]
fn project_pack_verifies_exact_external_package_trees() {
    let pack = sample_pack();
    let matching = cetz_bundle(b"");
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

    let mismatched_environment = pack
        .preparation_environment()
        .expect("base environment should build")
        .to_builder()
        .package_bundle(cetz_bundle(b"changed"))
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
        .external_font_file(external_font.clone())
        .font_file(embedded_font.clone())
        .build()
        .expect("pack with an external font should build");

    assert_eq!(pack.external_font_requirements().len(), 1);
    let requirement = &pack.external_font_requirements()[0];
    assert_eq!(
        requirement.container_digest(),
        *typst_pack::Digest::of(&external_font).as_bytes()
    );
    assert_eq!(requirement.face_indices(), [0]);
    let raw = decode_pack(&pack.to_bytes().expect("pack should serialize"));
    assert!(!raw.font_requirements()[0].embedded);
    assert!(raw.font_requirements()[1].embedded);
    assert_eq!(
        pack.render_environment(),
        Err(ProjectPackError::MissingExternalFont {
            container_digest: requirement.container_digest(),
            families: requirement.families().to_vec(),
        })
    );

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
fn project_pack_builder_rejects_a_package_manifest_naming_another_package() {
    let mislabeled =
        PackageBundle::builder("@demo/badge:0.1.0".parse().expect("spec should parse"))
            .file("typst.toml", package_manifest("other", "0.1.0"))
            .file("lib.typ", b"".to_vec())
            .build()
            .expect("bundle should build");

    assert!(matches!(
        ProjectPack::builder(Project::from_source("Hello"))
            .package_bundle(mislabeled)
            .build(),
        Err(ProjectPackError::Archive { .. })
    ));
}

#[test]
fn project_pack_reads_archives_written_by_typst_pack_directly() {
    // Interop guard: a pack assembled with the raw typst-pack builder,
    // not just our own writer, converts into crate domain types.
    let pack = typst_pack::archive::encode(
        &typst_pack::Pack::builder("main.typ")
            .file("main.typ", b"Hello".to_vec())
            .build()
            .expect("raw pack should build"),
    );

    let read = ProjectPack::from_bytes(&pack).expect("raw pack should parse");

    assert_eq!(read.project().root_path().get_without_slash(), "main.typ");
    assert_eq!(
        read.project().file_bytes("main.typ"),
        Some(b"Hello".as_slice())
    );
}

/// A golden format-version-2 `.typk` archive, written by typst-pack 0.6.0.
///
/// This pins the decoder against a fixed archive rather than against whatever
/// the current encoder happens to emit, so a format-level regression in any
/// future typst-pack upgrade fails here. It carries one embedded package, one
/// external package, and one external font container (Libertinus Serif from
/// `typst-assets`), so the recorded package-tree and font-container digests
/// are also checked against what this crate computes for the same resources.
const GOLDEN_PACK: &[u8] = include_bytes!("testdata/typst-pack-0.6.0.typk");

#[test]
fn a_golden_version_2_archive_still_reads() {
    let pack = ProjectPack::from_bytes(GOLDEN_PACK).expect("a version-2 archive should read");

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

    let fonts = pack.external_font_requirements();
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].families(), ["Libertinus Serif"]);
    assert_eq!(fonts[0].face_indices(), [0]);
    assert!(pack.font_files().is_empty());

    let metadata = pack.metadata().expect("the archive carries metadata");
    assert_eq!(metadata.name(), Some("Golden fixture"));
    assert_eq!(metadata.authors(), ["diotypst"]);

    assert_eq!(
        pack.to_bytes().expect("a read pack should serialize"),
        GOLDEN_PACK
    );
}

#[test]
fn the_golden_archive_digests_match_resources_computed_today() {
    // A changed package-tree or font-container digest would silently
    // invalidate every external requirement in an existing pack, so the
    // resources the golden archive was written from must still fulfill it.
    let pack = ProjectPack::from_bytes(GOLDEN_PACK).expect("a version-2 archive should read");
    let font = typst_assets::fonts()
        .next()
        .expect("bundled fonts should not be empty")
        .to_vec();

    let environment = pack
        .environment_builder()
        .package_bundle(cetz_bundle(b""))
        .font_file(font.clone())
        .build()
        .expect("the original resources should fulfill the golden pack");

    assert_eq!(
        environment
            .font_set()
            .font_store()
            .font(0)
            .expect("the external font should be installed")
            .data()
            .as_slice(),
        font
    );
}
