use std::{
    collections::HashMap,
    ops,
    path::{Path, PathBuf},
    range::Range,
    rc::Rc,
};

use ariadne::{Cache, Label, Report};

use crate::sourcemap::{SourceFileId, SourceMap, Span};

pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub priority: i32,
}

impl Diagnostic {
    pub fn new(span: Span, message: String) -> Self {
        Self {
            span,
            message,
            priority: 0,
        }
    }

    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

// Shared trait
pub trait AsDiagnostic {
    fn as_diagnostic(&self) -> Diagnostic;
}

pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Into<ariadne::ReportKind<'_>> for Severity {
    fn into(self) -> ariadne::ReportKind<'static> {
        match self {
            Severity::Error => ariadne::ReportKind::Error,
            Severity::Warning => ariadne::ReportKind::Warning,
            Severity::Info => ariadne::ReportKind::Advice,
        }
    }
}

pub struct AriadneCache {
    map: HashMap<Box<Path>, SourceFileId>,
    sources: HashMap<SourceFileId, ariadne::Source<String>>,
}

/// A report builder created by a SourceMap. Makes an ariadne report.
pub struct ReportBuilder<'map> {
    source_map: &'map mut SourceMap,
    diagnostics: Vec<Diagnostic>,
    primary_file: Option<SourceFileId>,
    hint: Vec<String>,
}

impl<'map> ReportBuilder<'map> {
    pub fn new(source_map: &'map mut SourceMap) -> Self {
        Self {
            source_map,
            diagnostics: Vec::new(),
            primary_file: None,
            hint: Vec::new(),
        }
    }

    pub fn as_cache(&self) -> AriadneCache {
        let sources = self
            .source_map
            .iter_files()
            .map(|(id, file)| {
                let source = ariadne::Source::from(file.source.clone());
                (id, source)
            })
            .collect();

        AriadneCache {
            sources,
            map: self.source_map.as_map().clone(),
        }
    }

    pub fn with_primary_file(mut self, file: SourceFileId) -> Self {
        self.primary_file = Some(file);
        self
    }

    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
    }

    pub fn with_diagnostics(
        mut self,
        diagnostics: impl IntoIterator<Item = impl Into<Diagnostic>>,
    ) -> Self {
        self.diagnostics
            .extend(diagnostics.into_iter().map(|d| d.into()));
        self
    }

    pub fn with_hint(mut self, hint: String) -> Self {
        self.hint.push(hint);
        self
    }

    fn main_span(&self) -> Option<(&str, ops::Range<usize>)> {
        let Some(primary_file) = self.primary_file else {
            return None;
        };

        // Find all spans, combine and return
        let span = Span::try_from_iter(self.diagnostics.iter().filter_map(|diagnostic| {
            if diagnostic.span.file != primary_file {
                return None;
            }
            Some(diagnostic.span)
        }))
        .ok()?;

        let file = self.source_map.path(primary_file)?.to_str()?;

        Some((file, span.start as usize..span.end as usize))
    }

    /// Create an ariadne report.
    ///
    /// Does not consume it as ariadne's report requires a lifetime for custom
    /// report kinds.
    pub fn build(&self) -> Option<Report<'_, (&str, ops::Range<usize>)>> {
        let mut builder = Report::build(
            ariadne::ReportKind::Error,
            self.main_span().unwrap_or(("", 0..0)),
        );

        for Diagnostic {
            span,
            message,
            priority,
        } in &self.diagnostics
        {
            builder = builder.with_label(
                Label::new((self.source_map.path_str(span.file)?, span.into()))
                    .with_message(message)
                    .with_priority(*priority),
            );
        }

        Some(builder.finish())
    }
}

impl Cache<&str> for AriadneCache {
    type Storage = String;

    fn fetch(
        &mut self,
        id: &&str,
    ) -> Result<&ariadne::Source<Self::Storage>, impl std::fmt::Debug> {
        let file_id = self
            .map
            .get(Path::new(id))
            .ok_or_else(|| format!("File not found: {id}"))?;
        self.sources
            .get(&file_id)
            .ok_or_else(|| format!("File not found: {id}"))
    }

    fn display<'a>(&self, id: &'a &str) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(*id))
    }
}
