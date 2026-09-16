use std::{collections::HashMap, fmt::Display, hash::Hash, ops, path::Path};

use ariadne::{Cache, Label, Report};

use crate::sourcemap::{SourceFileId, SourceMap, Span, SpanRecorder};

#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub priority: i32,
    pub labels: Vec<DiagnosticLabel>,
    pub code: Option<usize>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            priority: 0,
            labels: Vec::new(),
            code: None,
            help: None,
        }
    }

    pub fn with_label(mut self, message: impl Into<String>, span: Span) -> Self {
        self.labels.push(DiagnosticLabel {
            message: message.into(),
            span,
        });
        self
    }

    pub fn with_code(mut self, code: Option<usize>) -> Self {
        self.code = code;
        self
    }

    pub fn with_help(mut self, help: Option<impl Into<String>>) -> Self {
        self.help = help.map(|h| h.into());
        self
    }

    pub fn with_label_from<T: Hash + Eq>(
        mut self,
        recorder: &SpanRecorder<T>,
        key: &T,
        message: impl Into<String>,
    ) -> Self {
        let span = recorder
            .get(key)
            .cloned()
            .unwrap_or_else(|| Span::new(SourceFileId(0), 0, 0));
        self.labels.push(DiagnosticLabel {
            message: message.into(),
            span,
        });
        self
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

pub trait Diagnosable {
    fn diagnose(&self, message: String) -> Diagnostic;
}

impl Diagnosable for Span {
    fn diagnose(&self, message: String) -> Diagnostic {
        Diagnostic::new(message).with_label("the error occured here", *self)
    }
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

    pub fn with_diagnostic(mut self, diagnostic: impl Into<Diagnostic>) -> Self {
        self.diagnostics.push(diagnostic.into());
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
        let span = Span::try_from_iter(
            self.diagnostics
                .iter()
                .flat_map(|d| d.labels.clone())
                .filter_map(|diagnostic| {
                    if diagnostic.span.file != primary_file {
                        return None;
                    }
                    Some(diagnostic.span)
                }),
        )
        .ok()?;

        let file = self.source_map.path(primary_file)?.to_str()?;

        Some((file, span.start as usize..span.end as usize))
    }

    /// Create an ariadne report.
    ///
    /// Does not consume it as ariadne's report requires a lifetime for custom
    /// report kinds.
    pub fn build(&self) -> Vec<Report<'_, (&str, ops::Range<usize>)>> {
        let mut reports = Vec::new();
        for Diagnostic {
            labels,
            message,
            priority,
            code,
            help,
        } in &self.diagnostics
        {
            let mut builder = Report::build(
                ariadne::ReportKind::Error,
                self.main_span().unwrap_or(("", 0..0)),
            );
            builder = builder
                .with_message(message)
                .with_labels(labels.iter().map(|d| {
                    Label::new((
                        self.source_map.path_str(d.span.file).unwrap_or_default(),
                        (&d.span).into(),
                    ))
                    .with_message(&d.message)
                    .with_priority(*priority)
                }));

            if let Some(code) = code {
                builder = builder.with_code(*code);
            }

            if let Some(help) = help {
                builder = builder.with_help(help);
            }

            reports.push(builder.finish());
        }

        reports
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

    fn display<'a>(&self, id: &'a &str) -> std::option::Option<impl std::fmt::Display + 'a> {
        Some(Box::new(*id))
    }
}
