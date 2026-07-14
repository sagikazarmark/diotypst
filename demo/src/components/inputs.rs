//! Browser input controls shared by interactive examples.

use dioxus::prelude::*;

/// Which browser file selection flow a [`FilePickerButton`] opens.
#[derive(Clone, PartialEq)]
pub enum FilePickerKind {
    Single,
    Multiple,
    Directory,
}

/// Visual emphasis for a reusable input control.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ControlEmphasis {
    #[default]
    Neutral,
    Primary,
}

/// A button-styled browser file picker that emits the selected files directly.
#[component]
pub fn FilePickerButton(
    #[props(into)] label: String,
    kind: FilePickerKind,
    #[props(into, default)] accept: Option<String>,
    #[props(default)] emphasis: ControlEmphasis,
    onpick: EventHandler<Vec<dioxus::html::FileData>>,
) -> Element {
    let class = match emphasis {
        ControlEmphasis::Neutral => {
            "btn btn-sm focus-within:outline-2 focus-within:outline-offset-2"
        }
        ControlEmphasis::Primary => {
            "btn btn-sm btn-primary focus-within:outline-2 focus-within:outline-offset-2"
        }
    };
    let input = match kind {
        FilePickerKind::Single => rsx! {
            input {
                class: "sr-only",
                r#type: "file",
                accept: accept.unwrap_or_default(),
                onchange: move |event: FormEvent| onpick.call(event.files()),
            }
        },
        FilePickerKind::Multiple => rsx! {
            input {
                class: "sr-only",
                r#type: "file",
                accept: accept.unwrap_or_default(),
                multiple: true,
                onchange: move |event: FormEvent| onpick.call(event.files()),
            }
        },
        FilePickerKind::Directory => rsx! {
            input {
                class: "sr-only",
                r#type: "file",
                accept: accept.unwrap_or_default(),
                multiple: true,
                "webkitdirectory": "true",
                "directory": "true",
                onchange: move |event: FormEvent| onpick.call(event.files()),
            }
        },
    };

    rsx! {
        label { class, "{label}", {input} }
    }
}
