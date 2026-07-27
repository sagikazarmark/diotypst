use typst::foundations::Bytes;
use typst::text::{Font, FontInfo};
use typst_kit::fonts::{FontSource, FontStore};

use std::sync::{Arc, OnceLock};

/// An explicit collection of fonts available while rendering a Typst Project.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone)]
pub struct FontSet {
    source: FontSetSource,
    #[cfg_attr(feature = "serde", serde(skip, default = "empty_font_store_cache"))]
    store: Arc<OnceLock<FontStore>>,
}

impl FontSet {
    /// Use the fonts bundled with Typst.
    ///
    /// This is the default Font Set when the `bundled-fonts` feature is enabled.
    #[cfg(feature = "bundled-fonts")]
    pub fn bundled() -> Self {
        Self {
            source: FontSetSource::Bundled,
            store: empty_font_store_cache(),
        }
    }

    /// Create a Font Set with no fonts.
    pub fn empty() -> Self {
        Self {
            source: FontSetSource::Files(Arc::default()),
            store: empty_font_store_cache(),
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
            store: empty_font_store_cache(),
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
            store: empty_font_store_cache(),
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
            FontSetSource::Faces(faces) => FontSetSource::Faces(
                faces
                    .iter()
                    .cloned()
                    .chain(additional.into_iter().flat_map(|bytes| {
                        let face_count = FontInfo::iter(&bytes).count() as u32;
                        (0..face_count).map(move |index| FontFace {
                            bytes: Arc::clone(&bytes),
                            index,
                        })
                    }))
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

        Self {
            source,
            store: empty_font_store_cache(),
        }
    }

    /// Build a lazily-loading typst-kit font store for this Font Set.
    ///
    /// Face metadata is parsed eagerly (it feeds the `FontBook`), but full fonts load
    /// on first use through the store's slots.
    pub(crate) fn font_store(&self) -> &FontStore {
        self.store.get_or_init(|| {
            let mut store = FontStore::new();

            match &self.source {
                #[cfg(feature = "bundled-fonts")]
                FontSetSource::Bundled => store.extend(typst_kit::fonts::embedded()),
                FontSetSource::Files(font_files) => extend_with_font_files(&mut store, font_files),
                FontSetSource::Faces(faces) => extend_with_font_faces(&mut store, faces),
                #[cfg(feature = "bundled-fonts")]
                FontSetSource::BundledPlusFiles(font_files) => {
                    store.extend(typst_kit::fonts::embedded());
                    extend_with_font_files(&mut store, font_files);
                }
            }

            store
        })
    }

    #[cfg_attr(not(feature = "pack"), allow(dead_code))]
    pub(crate) fn from_font_faces(faces: impl IntoIterator<Item = (Vec<u8>, u32)>) -> Self {
        Self {
            source: FontSetSource::Faces(
                faces
                    .into_iter()
                    .map(|(bytes, index)| FontFace {
                        bytes: Arc::from(bytes),
                        index,
                    })
                    .collect::<Vec<_>>()
                    .into(),
            ),
            store: empty_font_store_cache(),
        }
    }

    #[cfg_attr(not(feature = "pack"), allow(dead_code))]
    pub(crate) fn container_files_where(
        &self,
        mut predicate: impl FnMut(&[u8]) -> bool,
    ) -> Vec<Vec<u8>> {
        let store = self.font_store();
        let mut seen = std::collections::HashSet::new();
        let mut files = Vec::new();
        let mut index = 0;
        while let Some(font) = store.font(index) {
            let identity = typst::utils::hash128(font.data());
            if seen.insert(identity) && predicate(font.data()) {
                files.push(font.data().to_vec());
            }
            index += 1;
        }
        files
    }
}

impl std::fmt::Debug for FontSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FontSet")
            .field("source", &self.source)
            .finish()
    }
}

impl PartialEq for FontSet {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for FontSet {}

fn empty_font_store_cache() -> Arc<OnceLock<FontStore>> {
    Arc::new(OnceLock::new())
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
    #[allow(dead_code)]
    Faces(Arc<[FontFace]>),
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct FontFace {
    bytes: Arc<[u8]>,
    index: u32,
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

fn extend_with_font_faces(store: &mut FontStore, faces: &[FontFace]) {
    for face in faces {
        let Some(info) = FontInfo::new(&face.bytes, face.index) else {
            continue;
        };
        store.push((
            BytesFace {
                bytes: Bytes::new(Arc::clone(&face.bytes)),
                index: face.index,
            },
            info,
        ));
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
            #[cfg(not(feature = "bundled-fonts"))]
            (FontSetSource::Faces(_), _) | (_, FontSetSource::Faces(_)) => {
                panic!("an empty Font Set extended with files should remain file-backed")
            }
        };

        assert!(Arc::ptr_eq(files, cloned_files));
        assert!(std::ptr::eq(font_set.font_store(), cloned.font_store()));
        assert!(font_set.font_store().book().info(0).is_some());
    }
}
