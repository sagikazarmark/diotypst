//! Router and app-wide Typst provider configuration.

#[cfg(target_arch = "wasm32")]
use diotypst::FontSet;
use diotypst::{
    FetchPackageSource, GatedPackages, RenderEnvironment, SharedPackageSource, TypstProvider,
    TypstProviderDefaults,
};
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

use crate::components::{DemoFooter, DemoHeader, Sidebar, SidebarNavLink, SidebarNavSection};
use crate::demo_package_policy;
use crate::pages::*;

const STYLE: Asset = asset!("/build/style.css");

#[cfg(target_arch = "wasm32")]
const FONT_URLS: &[&str] = &[
    "/fonts/LibertinusSerif-Regular.otf",
    "/fonts/LibertinusSerif-Bold.otf",
    "/fonts/LibertinusSerif-Italic.otf",
    "/fonts/LibertinusSerif-BoldItalic.otf",
    "/fonts/LibertinusSerif-Semibold.otf",
    "/fonts/LibertinusSerif-SemiboldItalic.otf",
    "/fonts/NewCMMath-Bold.otf",
    "/fonts/NewCMMath-Book.otf",
    "/fonts/NewCMMath-Regular.otf",
    "/fonts/NewCM10-Regular.otf",
    "/fonts/NewCM10-Bold.otf",
    "/fonts/NewCM10-Italic.otf",
    "/fonts/NewCM10-BoldItalic.otf",
    "/fonts/DejaVuSansMono-Bold.ttf",
    "/fonts/DejaVuSansMono-BoldOblique.ttf",
    "/fonts/DejaVuSansMono-Oblique.ttf",
    "/fonts/DejaVuSansMono.ttf",
];

#[cfg(target_arch = "wasm32")]
async fn load_font_set() -> Result<FontSet, String> {
    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_owned())?;
    let requests = js_sys::Array::new();
    for url in FONT_URLS {
        requests.push(&window.fetch_with_str(url));
    }

    let responses = JsFuture::from(js_sys::Promise::all(&requests))
        .await
        .map_err(|error| format!("font fetch failed: {error:?}"))?;
    let responses = js_sys::Array::from(&responses);
    let buffers = js_sys::Array::new();

    for (url, response) in FONT_URLS.iter().zip(responses.iter()) {
        let response: web_sys::Response = response
            .dyn_into()
            .map_err(|_| format!("font fetch for {url} did not return a Response"))?;
        if !response.ok() {
            return Err(format!(
                "font fetch for {url} returned HTTP {}",
                response.status()
            ));
        }
        let buffer = response
            .array_buffer()
            .map_err(|error| format!("reading font {url} failed: {error:?}"))?;
        buffers.push(&buffer.into());
    }

    let buffers = JsFuture::from(js_sys::Promise::all(&buffers))
        .await
        .map_err(|error| format!("reading fonts failed: {error:?}"))?;
    Ok(FontSet::from_font_files(
        js_sys::Array::from(&buffers)
            .iter()
            .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec()),
    ))
}

fn provider_defaults(environment: RenderEnvironment) -> TypstProviderDefaults {
    TypstProviderDefaults::new(environment).with_package_source(SharedPackageSource::new(
        GatedPackages::new(FetchPackageSource::proxy(), demo_package_policy()),
    ))
}

/// Every page hangs off the one `DemoLayout`, so the header, sidebar, and footer
/// render once and the active page swaps in through the `Outlet`.
#[derive(Routable, Clone, PartialEq, Debug)]
pub enum Route {
    #[layout(DemoLayout)]
    #[route("/")]
    Home {},
    // Basics
    #[route("/minimal")]
    Minimal {},
    #[route("/editor")]
    Editor {},
    #[route("/sys-inputs")]
    SysInputs {},
    // Typst Projects
    #[route("/projects/multi-file")]
    MultiFile {},
    #[route("/projects/import")]
    ImportProject {},
    #[route("/projects/pack")]
    ProjectPacks {},
    // Packages
    #[route("/packages/universe")]
    UniversePackages {},
    #[route("/packages/embedded")]
    EmbeddedPackage {},
    #[route("/packages/policy")]
    PackagePolicyPage {},
    // Downloads
    #[route("/downloads/pdf")]
    PdfDownload {},
    #[route("/downloads/pages")]
    PageImagesDownload {},
    // Server
    #[route("/server")]
    ServerRendering {},
}

/// Shared application shell for every demo route.
#[component]
fn DemoLayout() -> Element {
    rsx! {
        div { class: "min-h-screen bg-base-100 text-base-content",
            DemoHeader {
                home: Route::Home {},
                mark: "dt",
                name: "diotypst",
                github_url: "https://github.com/sagikazarmark/diotypst",
            }
            div { class: "mx-auto w-full max-w-7xl lg:flex lg:gap-8 lg:px-6",
                Sidebar {
                    SidebarNavSection { label: "Basics",
                        SidebarNavLink { route: Route::Home {}, label: "Overview" }
                        SidebarNavLink { route: Route::Minimal {}, label: "Minimal render" }
                        SidebarNavLink { route: Route::Editor {}, label: "Editor & live preview" }
                        SidebarNavLink { route: Route::SysInputs {}, label: "System inputs" }
                    }
                    SidebarNavSection { label: "Typst Projects",
                        SidebarNavLink { route: Route::MultiFile {}, label: "Multi-file project" }
                        SidebarNavLink { route: Route::ImportProject {}, label: "Import files & fonts" }
                        SidebarNavLink { route: Route::ProjectPacks {}, label: "Project packs (.typk)" }
                    }
                    SidebarNavSection { label: "Packages",
                        SidebarNavLink { route: Route::UniversePackages {}, label: "Typst Universe" }
                        SidebarNavLink { route: Route::EmbeddedPackage {}, label: "Embedded package" }
                        SidebarNavLink { route: Route::PackagePolicyPage {}, label: "Package policy" }
                    }
                    SidebarNavSection { label: "Downloads",
                        SidebarNavLink { route: Route::PdfDownload {}, label: "PDF" }
                        SidebarNavLink { route: Route::PageImagesDownload {}, label: "Page images" }
                    }
                    SidebarNavSection { label: "Server",
                        SidebarNavLink { route: Route::ServerRendering {}, label: "Server rendering" }
                    }
                }
                main { id: "main-content", class: "min-w-0 flex-1 px-4 py-8 sm:px-6 lg:px-0 lg:py-12",
                    Outlet::<Route> {}
                }
            }
            DemoFooter {
                description: "A docs-by-example gallery for the diotypst library.",
                links: rsx! {
                    a {
                        class: "hover:text-base-content",
                        href: "https://github.com/sagikazarmark/diotypst",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "Repository"
                    }
                },
            }
        }
    }
}

#[component]
pub fn App() -> Element {
    #[cfg(target_arch = "wasm32")]
    let fonts = use_resource(load_font_set);

    #[cfg(target_arch = "wasm32")]
    let defaults = {
        let fonts = fonts.read();
        let Some(fonts) = fonts.as_ref() else {
            return rsx! {
                document::Stylesheet { href: STYLE }
                main { class: "grid min-h-screen place-items-center bg-base-100 p-6 text-base-content",
                    p { class: "text-sm text-base-content/70", "Loading Typst fonts..." }
                }
            };
        };
        let font_set = match fonts {
            Ok(font_set) => font_set.clone(),
            Err(error) => {
                return rsx! {
                    document::Stylesheet { href: STYLE }
                    main { class: "grid min-h-screen place-items-center bg-base-100 p-6 text-base-content",
                        div { class: "alert alert-error max-w-xl",
                            span { "Could not load Typst fonts: {error}" }
                        }
                    }
                };
            }
        };
        let environment = RenderEnvironment::builder()
            .font_set(font_set)
            .build()
            .expect("the fetched font set should produce a valid render environment");
        provider_defaults(environment)
    };

    #[cfg(not(target_arch = "wasm32"))]
    let defaults = provider_defaults(RenderEnvironment::default());

    rsx! {
        document::Stylesheet { href: STYLE }
        TypstProvider { defaults,
            Router::<Route> {}
        }
    }
}
