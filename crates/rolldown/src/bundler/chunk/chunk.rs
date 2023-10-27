use oxc::span::Atom;
use rolldown_common::{ModuleId, SymbolRef};
use rustc_hash::FxHashMap;
use string_wizard::{Joiner, JoinerOptions};

use crate::bundler::{
  bitset::BitSet,
  chunk_graph::ChunkGraph,
  graph::graph::Graph,
  module::ModuleRenderContext,
  options::{
    file_name_template::FileNameRenderOptions, normalized_output_options::NormalizedOutputOptions,
  },
};

use super::ChunkId;

#[derive(Debug)]
pub struct CrossChunkImportItem {
  pub export_alias: Option<Atom>,
  pub import_ref: SymbolRef,
}

#[derive(Debug, Default)]
pub struct Chunk {
  pub entry_module: Option<ModuleId>,
  pub modules: Vec<ModuleId>,
  pub name: Option<String>,
  pub file_name: Option<String>,
  pub canonical_names: FxHashMap<SymbolRef, Atom>,
  pub bits: BitSet,
  pub imports_from_other_chunks: FxHashMap<ChunkId, Vec<CrossChunkImportItem>>,
  // meaningless if the chunk is an entrypoint
  pub exports_to_other_chunks: FxHashMap<SymbolRef, Atom>,
}

impl Chunk {
  pub fn new(
    name: Option<String>,
    entry_module: Option<ModuleId>,
    bits: BitSet,
    modules: Vec<ModuleId>,
  ) -> Self {
    Self { entry_module, modules, name, bits, ..Self::default() }
  }

  pub fn render_file_name(&mut self, output_options: &NormalizedOutputOptions) {
    self.file_name = Some(
      output_options.entry_file_names.render(&FileNameRenderOptions { name: self.name.as_deref() }),
    );
  }

  #[allow(clippy::unnecessary_wraps)]
  pub fn render(&self, graph: &Graph, chunk_graph: &ChunkGraph) -> anyhow::Result<String> {
    use rayon::prelude::*;
    let mut joiner = Joiner::with_options(JoinerOptions { separator: Some("\n".to_string()) });
    joiner.append(self.render_imports_for_esm(graph, chunk_graph));
    self
      .modules
      .par_iter()
      .copied()
      .map(|id| &graph.modules[id])
      .filter_map(|m| {
        m.render(ModuleRenderContext { canonical_names: &self.canonical_names, graph, chunk_graph })
      })
      .collect::<Vec<_>>()
      .into_iter()
      .for_each(|item| {
        joiner.append(item);
      });

    if let Some(exports) = self.render_exports_for_esm(graph) {
      joiner.append(exports);
    }

    Ok(joiner.join())
  }
}
