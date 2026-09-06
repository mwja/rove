use std::{collections::HashMap, ops, path::Path, range::Range, rc::Rc};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceFileId(pub u32);

/// Stores a span and a file reference. If you need to create this, use
/// the rust-sitter feature and [SourceMap::load]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: SourceFileId,
    pub start: u32,
    pub end: u32,
}

impl Into<ops::Range<usize>> for &Span {
    fn into(self) -> ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

impl Span {
    pub fn new(file: SourceFileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    pub fn combine(&self, other: &Span) -> Option<Span> {
        if self.file != other.file {
            return None;
        }
        let start = self.start.min(other.start);
        let end = self.end.max(other.end);
        Some(Span {
            file: self.file,
            start,
            end,
        })
    }

    pub fn length(&self) -> u32 {
        self.end - self.start
    }

    pub fn from_span(source: SourceFileId, value: (usize, usize)) -> Self {
        Span::new(source, value.0 as u32, value.1 as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }
}

pub struct SourceFile {
    pub path: Box<Path>,
    pub source: String,
    pub length: usize,
}

impl SourceFile {
    pub fn new(path: Box<Path>, source: String) -> Self {
        let length = source.len();
        Self {
            path,
            source,
            length,
        }
    }
}

pub struct SourceMap {
    files: Vec<SourceFile>,
    map: HashMap<Box<Path>, SourceFileId>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            map: HashMap::new(),
        }
    }

    pub fn as_map(&self) -> &HashMap<Box<Path>, SourceFileId> {
        &self.map
    }

    pub fn get_file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn get_file_from_path(&self, path: &Path) -> Option<&SourceFile> {
        self.map.get(path).and_then(|id| self.get_file(*id))
    }

    pub fn get_file_id(&self, path: &Path) -> Option<SourceFileId> {
        self.map.get(path).copied()
    }

    pub fn iter_files(&self) -> impl Iterator<Item = (SourceFileId, &SourceFile)> {
        self.files
            .iter()
            .enumerate()
            .map(|(i, file)| (SourceFileId(i as u32), file))
    }

    pub fn add_file(&mut self, path: Box<Path>, source: String) -> SourceFileId {
        if let Some(id) = self.map.get(&path) {
            return *id;
        }
        let id = SourceFileId(self.files.len() as u32);
        let length = source.len();
        self.files.push(SourceFile {
            path: path.clone(),
            source,
            length,
        });
        self.map.insert(path, id);
        id
    }

    pub fn file(&self, path: &Path) -> Option<&SourceFileId> {
        self.map.get(path)
    }

    pub fn path(&self, id: SourceFileId) -> Option<&Box<Path>> {
        self.files.get(id.0 as usize).map(|f| &f.path)
    }

    pub fn path_str(&self, id: SourceFileId) -> Option<&str> {
        self.files
            .get(id.0 as usize)
            .map(|f| f.path.to_str())
            .flatten()
    }

    fn span_to_line_and_column(&self, span: Span) -> Option<((usize, usize), (usize, usize))> {
        let file = self.files.get(span.file.0 as usize)?;
        let source = &file.source;

        let mut line_start = 0;
        let mut line_number = 1;
        let mut start_line = 0;
        let mut start_column = 0;
        let mut end_line = 0;
        let mut end_column = 0;

        for (i, c) in source.char_indices() {
            if i == span.start as usize {
                start_line = line_number;
                start_column = i - line_start + 1; // +1 for 1-based indexing
            }
            if i == span.end as usize {
                end_line = line_number;
                end_column = i - line_start + 1; // +1 for 1-based indexing
                break;
            }
            if c == '\n' {
                line_start = i + 1;
                line_number += 1;
            }
        }

        Some(((start_line, start_column), (end_line, end_column)))
    }

    /// Describe the given span in a string, of format `filename: start_line:start_column-end_line:end_column`
    pub fn describe_span(&self, span: Span) -> Option<String> {
        let file = self.files.get(span.file.0 as usize)?;
        let ((start_line, start_column), (end_line, end_column)) =
            self.span_to_line_and_column(span)?;

        Some(format!(
            "{}: {}:{}-{}:{}",
            file.path.display(),
            start_line,
            start_column,
            end_line,
            end_column
        ))
    }

    pub fn build_report<'map>(&'map mut self) -> crate::sourcemap::ReportBuilder<'map> {
        crate::sourcemap::ReportBuilder::new(self)
    }
}

impl FromIterator<SourceFile> for SourceMap {
    fn from_iter<T: IntoIterator<Item = SourceFile>>(iter: T) -> Self {
        let mut map = SourceMap::new();
        for file in iter {
            map.add_file(file.path, file.source);
        }
        map
    }
}

impl<const N: usize> From<[SourceFile; N]> for SourceMap {
    fn from(files: [SourceFile; N]) -> Self {
        files.into_iter().collect()
    }
}

#[derive(Debug, Error)]
pub enum SpanError {
    #[error("Empty iterator")]
    EmptyIterator,
    #[error("Spans are from different files")]
    DifferentFiles,
}

impl Span {
    pub fn try_from_iter<T>(iter: T) -> Result<Self, SpanError>
    where
        T: IntoIterator<Item = Span>,
    {
        let mut iter = iter.into_iter();

        let first = iter.next().ok_or(SpanError::EmptyIterator)?;

        iter.try_fold(first, |acc, span| {
            acc.combine(&span).ok_or(SpanError::DifferentFiles)
        })
    }
}
