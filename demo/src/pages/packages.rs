use dioxus::prelude::*;
use dioxus_code::{Code, code};

use crate::components::{DocsCallout, ExampleSection, InlineCode, PageHeader, snippet_theme};
use crate::examples::embedded::EmbeddedExample;
use crate::examples::policy::PolicyExample;
use crate::examples::universe::UniverseExample;

#[component]
pub fn UniversePackages() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Packages",
            title: "Typst Universe downloads",
            intro: "Packages resolve through explicit Package Sources during World Preparation, before world construction, and land in the Render Environment as in-memory Package Bundles. Rendering never fetches.",
        }
        ExampleSection {
            title: "use_render_session + World Preparation",
            intro: rsx! {
                "This project imports CetZ. The Render Session preflight-compiles the project, resolves the observed packages through the app-wide "
                InlineCode { "FetchPackageSource::proxy()" }
                " (the same-origin package proxy), and repeats until nothing is missing; packages can import further packages."
            },
            demo: rsx! { UniverseExample {} },
            code: rsx! {
                Code { src: code!("src/examples/universe.rs"), theme: snippet_theme() }
            },
        }
        DocsCallout {
            title: "Package sources & preparation",
            "Sources compose in ordered chains, any source can be policy-gated, and unresolved packages are recorded per spec instead of failing preparation; the subsequent render surfaces Typst's own package diagnostics."
        }
    }
}

#[component]
pub fn EmbeddedPackage() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Packages",
            title: "An embedded package",
            intro: "Verbatim Typst Universe .tar.gz archives parse back into Package Bundles anywhere, including wasm, so a package can ship inside the binary and render offline.",
        }
        ExampleSection {
            title: "PackageBundle::from_tar_gz + include_bytes!",
            intro: rsx! {
                "The "
                InlineCode { "@demo/demo-badge" }
                " archive is compiled into the app and installed into the Render Environment directly: no Package Source, no World Preparation, no network."
            },
            demo: rsx! { EmbeddedExample {} },
            code: rsx! {
                Code { src: code!("src/examples/embedded.rs"), theme: snippet_theme() }
            },
        }
    }
}

#[component]
pub fn PackagePolicyPage() -> Element {
    rsx! {
        PageHeader {
            eyebrow: "Packages",
            title: "Package policy",
            intro: "A PackagePolicy is a serializable allowlist/denylist over package specs, with deny patterns winning at namespace, name, or exact-version granularity. The demo denies everything except CetZ, its dependencies, and the embedded @demo namespace.",
        }
        ExampleSection {
            title: "GatedPackages + PackagePolicy",
            intro: rsx! {
                "This project imports a package outside the allowlist. The client-side gate reports it as "
                InlineCode { "denied" }
                " before any request as a fast path; the package proxy enforces the same policy authoritatively on the server (and in the Cloudflare Worker)."
            },
            demo: rsx! { PolicyExample {} },
            code: rsx! {
                Code { src: code!("src/examples/policy.rs"), theme: snippet_theme() }
            },
        }
    }
}
