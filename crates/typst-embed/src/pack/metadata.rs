/// Optional descriptive metadata carried by a Project Pack.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectPackMetadata {
    name: Option<String>,
    description: Option<String>,
    authors: Vec<String>,
}

impl ProjectPackMetadata {
    /// Create empty metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a display name for the packed project.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set a description for the packed project.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add an author of the packed project.
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.authors.push(author.into());
        self
    }

    /// Return the display name, if set.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Return the description, if set.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Return the authors.
    pub fn authors(&self) -> &[String] {
        &self.authors
    }
}
