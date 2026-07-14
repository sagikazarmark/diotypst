use crate::{DuplicatePackageSpec, FontSet, PackageBundle, PackageBundleSet, PackageSpec};
use typst::foundations::{Datetime, Dict, IntoValue};

const DEFAULT_RENDER_DATE: RenderDate = RenderDate {
    year: 2026,
    month: 7,
    day: 1,
};

/// A deterministic calendar date returned to Typst date-sensitive rendering.
///
/// The default Render Date is 2026-07-01.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderDate {
    year: i32,
    month: u8,
    day: u8,
}

impl RenderDate {
    /// Create a Render Date from a year, month, and day.
    pub fn from_ymd(year: i32, month: u8, day: u8) -> Option<Self> {
        Datetime::from_ymd(year, month, day)?;

        Some(Self { year, month, day })
    }

    /// Return the year.
    pub fn year(&self) -> i32 {
        self.year
    }

    /// Return the one-based month.
    pub fn month(&self) -> u8 {
        self.month
    }

    /// Return the one-based day of the month.
    pub fn day(&self) -> u8 {
        self.day
    }

    pub(crate) fn to_datetime(self) -> Datetime {
        Datetime::from_ymd(self.year, self.month, self.day)
            .expect("RenderDate should only contain a valid Typst date")
    }
}

impl Default for RenderDate {
    fn default() -> Self {
        DEFAULT_RENDER_DATE
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for RenderDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RenderDateFields {
            year: i32,
            month: u8,
            day: u8,
        }

        let fields = <RenderDateFields as serde::Deserialize>::deserialize(deserializer)?;

        Self::from_ymd(fields.year, fields.month, fields.day)
            .ok_or_else(|| serde::de::Error::custom("invalid Render Date"))
    }
}

/// Explicit non-source context used while rendering a Typst Project.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderEnvironment {
    package_bundles: PackageBundleSet,
    font_set: FontSet,
    render_date: RenderDate,
    inputs: Dict,
}

impl RenderEnvironment {
    /// Start building a render environment.
    pub fn builder() -> RenderEnvironmentBuilder {
        RenderEnvironmentBuilder {
            package_bundles: PackageBundleSet::new(),
            duplicate_package: None,
            font_set: FontSet::default(),
            render_date: RenderDate::default(),
            inputs: Dict::new(),
        }
    }

    /// Start building from this environment's current resources.
    pub fn to_builder(&self) -> RenderEnvironmentBuilder {
        RenderEnvironmentBuilder {
            package_bundles: self.package_bundles.clone(),
            duplicate_package: None,
            font_set: self.font_set.clone(),
            render_date: self.render_date,
            inputs: self.inputs.clone(),
        }
    }

    /// Return a resolved package bundle by exact package spec.
    pub fn package_bundle(&self, spec: &PackageSpec) -> Option<&PackageBundle> {
        self.package_bundles.get(spec)
    }

    /// Return the Font Set used while rendering.
    pub fn font_set(&self) -> &FontSet {
        &self.font_set
    }

    /// Return the deterministic Render Date used for Typst `datetime.today()`.
    pub fn render_date(&self) -> RenderDate {
        self.render_date
    }

    /// Return the Typst values visible through `sys.inputs`.
    pub fn inputs(&self) -> &Dict {
        &self.inputs
    }
}

impl Default for RenderEnvironment {
    /// The default Render Environment: no Package Bundles, the default Font Set for
    /// this build, the default Render Date, and no `sys.inputs`.
    fn default() -> Self {
        Self {
            package_bundles: PackageBundleSet::new(),
            font_set: FontSet::default(),
            render_date: RenderDate::default(),
            inputs: Dict::new(),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for RenderEnvironment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(serde::Serialize)]
        struct RenderEnvironmentWire<'a> {
            package_bundles: &'a [PackageBundle],
            font_set: &'a FontSet,
            render_date: RenderDate,
            inputs: &'a Dict,
        }

        RenderEnvironmentWire {
            package_bundles: self.package_bundles.bundles(),
            font_set: &self.font_set,
            render_date: self.render_date,
            inputs: &self.inputs,
        }
        .serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for RenderEnvironment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct RenderEnvironmentFields {
            #[serde(default)]
            package_bundles: Vec<PackageBundle>,
            #[serde(default)]
            font_set: FontSet,
            #[serde(default)]
            render_date: RenderDate,
            #[serde(default)]
            inputs: Dict,
        }

        let fields = <RenderEnvironmentFields as serde::Deserialize>::deserialize(deserializer)?;

        RenderEnvironment::builder()
            .package_bundles(fields.package_bundles)
            .font_set(fields.font_set)
            .render_date(fields.render_date)
            .inputs(fields.inputs)
            .build()
            .map_err(|error| {
                serde::de::Error::custom(format!("invalid Render Environment: {error:?}"))
            })
    }
}

/// Builder for a render environment.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderEnvironmentBuilder {
    package_bundles: PackageBundleSet,
    duplicate_package: Option<DuplicatePackageSpec>,
    font_set: FontSet,
    render_date: RenderDate,
    inputs: Dict,
}

impl RenderEnvironmentBuilder {
    /// Add a resolved package bundle to this environment.
    pub fn package_bundle(mut self, bundle: PackageBundle) -> Self {
        if let Err(duplicate) = self.package_bundles.try_insert(bundle) {
            self.duplicate_package.get_or_insert(duplicate);
        }
        self
    }

    /// Add resolved package bundles to this environment.
    pub fn package_bundles(mut self, bundles: impl IntoIterator<Item = PackageBundle>) -> Self {
        for bundle in bundles {
            self = self.package_bundle(bundle);
        }
        self
    }

    /// Add or replace a resolved package bundle by exact package spec.
    pub fn replace_package_bundle(mut self, bundle: PackageBundle) -> Self {
        self.package_bundles.upsert(bundle);
        self
    }

    /// Use an explicit Font Set for rendering in this environment.
    pub fn font_set(mut self, font_set: FontSet) -> Self {
        self.font_set = font_set;
        self
    }

    /// Use an explicit Render Date for Typst date-sensitive rendering.
    pub fn render_date(mut self, render_date: RenderDate) -> Self {
        self.render_date = render_date;
        self
    }

    /// Replace the Typst values visible through `sys.inputs`.
    pub fn inputs(mut self, inputs: Dict) -> Self {
        self.inputs = inputs;
        self
    }

    /// Merge Typst `sys.inputs` values into this environment; later keys replace earlier keys.
    pub fn merge_inputs(mut self, inputs: Dict) -> Self {
        self.inputs += inputs;
        self
    }

    /// Add or replace one Typst value visible through `sys.inputs`.
    pub fn input(mut self, key: impl Into<String>, value: impl IntoValue) -> Self {
        self.inputs.insert(key.into().into(), value.into_value());
        self
    }

    /// Build and validate the render environment.
    pub fn build(self) -> Result<RenderEnvironment, RenderEnvironmentError> {
        if let Some(duplicate) = self.duplicate_package {
            return Err(RenderEnvironmentError::DuplicatePackage {
                spec: duplicate.spec,
            });
        }

        Ok(RenderEnvironment {
            package_bundles: self.package_bundles,
            font_set: self.font_set,
            render_date: self.render_date,
            inputs: self.inputs,
        })
    }
}

/// A render environment validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderEnvironmentError {
    /// More than one Package Bundle has the same exact package spec.
    DuplicatePackage { spec: PackageSpec },
}
