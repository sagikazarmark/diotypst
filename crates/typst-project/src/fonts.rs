use typst::foundations::Bytes;
use typst::text::{Font, FontInfo};
use typst_kit::fonts::{FontSource, FontStore};

use std::sync::Arc;

/// An explicit collection of fonts available while rendering a Typst Project.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FontSet {
    source: FontSetSource,
}

impl FontSet {
    /// Use the fonts bundled with Typst.
    ///
    /// This is the default Font Set when the `bundled-fonts` feature is enabled.
    #[cfg(feature = "bundled-fonts")]
    pub fn bundled() -> Self {
        Self {
            source: FontSetSource::Bundled,
        }
    }

    /// Create a Font Set with no fonts.
    pub fn empty() -> Self {
        Self {
            source: FontSetSource::Files(Arc::default()),
        }
    }

    /// Create a Font Set from explicit font file bytes.
    ///
    /// Each file may contain one or more fonts. Files that Typst cannot parse as fonts contribute
    /// no fonts to the set.
    pub fn from_font_files<I, B>(font_files: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: Into<Vec<u8>>,
    {
        Self {
            source: FontSetSource::Files(
                font_files
                    .into_iter()
                    .map(|font| Arc::<[u8]>::from(font.into()))
                    .collect::<Vec<_>>()
                    .into(),
            ),
        }
    }

    /// Create a Font Set from bundled Typst fonts plus explicit font file bytes.
    #[cfg(feature = "bundled-fonts")]
    pub fn bundled_plus_font_files<I, B>(font_files: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: Into<Vec<u8>>,
    {
        Self {
            source: FontSetSource::BundledPlusFiles(
                font_files
                    .into_iter()
                    .map(|font| Arc::<[u8]>::from(font.into()))
                    .collect::<Vec<_>>()
                    .into(),
            ),
        }
    }

    /// Add explicit font file bytes to this Font Set.
    pub fn with_font_files<I, B>(self, font_files: I) -> Self
    where
        I: IntoIterator<Item = B>,
        B: Into<Vec<u8>>,
    {
        let additional = font_files
            .into_iter()
            .map(|font| Arc::<[u8]>::from(font.into()))
            .collect::<Vec<_>>();
        if additional.is_empty() {
            return self;
        }

        let source = match self.source {
            #[cfg(feature = "bundled-fonts")]
            FontSetSource::Bundled => FontSetSource::BundledPlusFiles(additional.into()),
            FontSetSource::Files(files) => FontSetSource::Files(
                files
                    .iter()
                    .cloned()
                    .chain(additional)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            #[cfg(feature = "bundled-fonts")]
            FontSetSource::BundledPlusFiles(files) => FontSetSource::BundledPlusFiles(
                files
                    .iter()
                    .cloned()
                    .chain(additional)
                    .collect::<Vec<_>>()
                    .into(),
            ),
        };

        Self { source }
    }

    /// Build a lazily-loading typst-kit font store for this Font Set.
    ///
    /// Face metadata is parsed eagerly (it feeds the `FontBook`), but full fonts load
    /// on first use through the store's slots.
    pub(crate) fn font_store(&self) -> FontStore {
        let mut store = FontStore::new();

        match &self.source {
            #[cfg(feature = "bundled-fonts")]
            FontSetSource::Bundled => store.extend(typst_kit::fonts::embedded()),
            FontSetSource::Files(font_files) => extend_with_font_files(&mut store, font_files),
            #[cfg(feature = "bundled-fonts")]
            FontSetSource::BundledPlusFiles(font_files) => {
                store.extend(typst_kit::fonts::embedded());
                extend_with_font_files(&mut store, font_files);
            }
        }

        store
    }
}

impl Default for FontSet {
    fn default() -> Self {
        #[cfg(feature = "bundled-fonts")]
        return Self::bundled();

        #[cfg(not(feature = "bundled-fonts"))]
        Self::empty()
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
enum FontSetSource {
    #[cfg(feature = "bundled-fonts")]
    Bundled,
    Files(Arc<[Arc<[u8]>]>),
    #[cfg(feature = "bundled-fonts")]
    BundledPlusFiles(Arc<[Arc<[u8]>]>),
}

fn extend_with_font_files(store: &mut FontStore, font_files: &[Arc<[u8]>]) {
    for data in font_files {
        let bytes = Bytes::new(Arc::clone(data));
        for (index, info) in FontInfo::iter(data.as_ref()).enumerate() {
            store.push((
                BytesFace {
                    bytes: bytes.clone(),
                    index: index as u32,
                },
                info,
            ));
        }
    }
}

/// One face of an in-memory font file, loaded on first use.
struct BytesFace {
    bytes: Bytes,
    index: u32,
}

impl FontSource for BytesFace {
    fn load(&self) -> Option<Font> {
        Font::new(self.bytes.clone(), self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::{FontSet, FontSetSource};
    use std::sync::Arc;

    #[test]
    fn adding_no_font_files_preserves_the_font_set() {
        let font_set = FontSet::default();

        assert_eq!(
            font_set.clone().with_font_files(Vec::<Vec<u8>>::new()),
            font_set
        );
    }

    #[test]
    fn explicit_font_files_survive_cheap_font_set_clones() {
        let font = typst_assets::fonts()
            .next()
            .expect("the test font feature should provide a font")
            .to_vec();
        let font_set = FontSet::empty().with_font_files([font]);
        let cloned = font_set.clone();

        let (files, cloned_files) = match (&font_set.source, &cloned.source) {
            (FontSetSource::Files(files), FontSetSource::Files(cloned_files)) => {
                (files, cloned_files)
            }
            #[cfg(feature = "bundled-fonts")]
            _ => panic!("an empty Font Set extended with files should remain file-backed"),
        };

        assert!(Arc::ptr_eq(files, cloned_files));
        assert!(font_set.font_store().book().info(0).is_some());
    }
}
