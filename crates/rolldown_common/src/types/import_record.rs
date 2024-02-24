use std::fmt::Display;

use oxc::span::Atom;

use crate::{ModuleId, SymbolRef};

index_vec::define_index_type! {
  pub struct ImportRecordId = u32;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ImportKind {
  Import,
  DynamicImport,
  Require,
}

impl ImportKind {
  pub fn is_static(&self) -> bool {
    matches!(self, Self::Import | Self::Require)
  }
}

impl Display for ImportKind {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Import => write!(f, "import-statement"),
      Self::DynamicImport => write!(f, "dynamic-import"),
      Self::Require => write!(f, "require-call"),
    }
  }
}

#[derive(Debug)]
pub struct RawImportRecord {
  // Module Request
  pub module_request: Atom,
  pub kind: ImportKind,
  pub namespace_ref: SymbolRef,
  pub contains_import_star: bool,
  pub contains_import_default: bool,
}

impl RawImportRecord {
  pub fn new(specifier: Atom, kind: ImportKind, namespace_ref: SymbolRef) -> Self {
    Self {
      module_request: specifier,
      kind,
      namespace_ref,
      contains_import_default: false,
      contains_import_star: false,
    }
  }

  pub fn into_import_record(self, resolved_module: ModuleId) -> ImportRecord {
    ImportRecord {
      module_request: self.module_request,
      resolved_module,
      kind: self.kind,
      namespace_ref: self.namespace_ref,
      contains_import_star: self.contains_import_star,
      contains_import_default: self.contains_import_default,
    }
  }
}

#[derive(Debug)]
pub struct ImportRecord {
  // Module Request
  pub module_request: Atom,
  pub resolved_module: ModuleId,
  pub kind: ImportKind,
  pub namespace_ref: SymbolRef,
  pub contains_import_star: bool,
  pub contains_import_default: bool,
}