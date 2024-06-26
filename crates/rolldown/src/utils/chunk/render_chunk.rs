use std::path::PathBuf;

use crate::{
  chunk_graph::ChunkGraph, stages::link_stage::LinkStageOutput,
  types::module_render_output::ModuleRenderOutput,
  utils::render_normal_module::render_normal_module, SharedOptions,
};

use anyhow::Result;
use rolldown_common::{
  Chunk, ChunkKind, ExportsKind, OutputFormat, RenderedChunk, ResourceId, WrapKind,
};
use rolldown_sourcemap::{ConcatSource, RawSource, SourceMap, SourceMapSource};
use rolldown_utils::rayon::{IntoParallelRefIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use sugar_path::SugarPath;

pub struct ChunkRenderReturn {
  pub code: String,
  pub map: Option<SourceMap>,
  pub rendered_chunk: RenderedChunk,
  pub augment_chunk_hash: Option<String>,
  pub file_dir: PathBuf,
  pub preliminary_filename: ResourceId,
}

use super::{
  generate_rendered_chunk, render_chunk_exports::render_chunk_exports,
  render_chunk_imports::render_chunk_imports,
};

#[allow(clippy::unnecessary_wraps, clippy::cast_possible_truncation)]
#[tracing::instrument(level = "trace", skip_all)]
pub async fn render_chunk(
  this: &Chunk,
  options: &SharedOptions,
  graph: &LinkStageOutput,
  chunk_graph: &ChunkGraph,
) -> Result<ChunkRenderReturn> {
  let mut rendered_modules = FxHashMap::default();
  let mut concat_source = ConcatSource::default();

  let rendered_chunk = match options.format {
    OutputFormat::Esm | OutputFormat::Cjs => {
      concat_source.add_source(Box::new(RawSource::new(render_chunk_imports(
        this,
        graph,
        chunk_graph,
        options,
      ))));

      this
        .modules
        .par_iter()
        .copied()
        .map(|id| &graph.module_table.normal_modules[id])
        .filter_map(|m| {
          render_normal_module(m, &graph.ast_table[m.id], m.resource_id.as_ref(), options)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|module_render_output| {
          let ModuleRenderOutput {
            module_path,
            module_pretty_path,
            rendered_module,
            rendered_content,
            sourcemap,
            lines_count,
          } = module_render_output;
          concat_source.add_source(Box::new(RawSource::new(format!("// {module_pretty_path}",))));
          if let Some(sourcemap) = sourcemap {
            concat_source.add_source(Box::new(SourceMapSource::new(
              rendered_content,
              sourcemap,
              lines_count,
            )));
          } else {
            concat_source.add_source(Box::new(RawSource::new(rendered_content)));
          }
          // FIXME: NAPI-RS used CStr under the hood, so it can't handle null byte in the string.
          if !module_path.starts_with('\0') {
            rendered_modules.insert(module_path, rendered_module);
          }
        });

      generate_rendered_chunk(this, graph, options, rendered_modules, chunk_graph)
    }
    OutputFormat::App => {
      this
        .modules
        .par_iter()
        .copied()
        .map(|id| &graph.module_table.normal_modules[id])
        .filter_map(|m| {
          render_normal_module(m, &graph.ast_table[m.id], m.resource_id.as_ref(), options)
        })
        .collect::<Vec<_>>()
        .into_iter()
        .for_each(|module_render_output| {
          let ModuleRenderOutput {
            module_path,
            module_pretty_path,
            rendered_module,
            rendered_content,
            sourcemap,
            lines_count,
          } = module_render_output;
          concat_source.add_source(Box::new(RawSource::new(format!("// {module_pretty_path}",))));
          if let Some(sourcemap) = sourcemap {
            concat_source.add_source(Box::new(SourceMapSource::new(
              rendered_content,
              sourcemap,
              lines_count,
            )));
          } else {
            concat_source.add_source(Box::new(RawSource::new(rendered_content)));
          }
          // FIXME: NAPI-RS used CStr under the hood, so it can't handle null byte in the string.
          if !module_path.starts_with('\0') {
            rendered_modules.insert(module_path, rendered_module);
          }
        });
      generate_rendered_chunk(this, graph, options, rendered_modules, chunk_graph)
    }
  };

  // add banner
  if let Some(banner) = options.banner.as_ref() {
    if let Some(banner_txt) = banner.call(&rendered_chunk).await? {
      if !banner_txt.is_empty() {
        concat_source.add_prepend_source(Box::new(RawSource::new(banner_txt)));
      }
    }
  }

  // Add `use strict` directive if needed. This must come before the banner, because users might use banner to add hashbang.
  if matches!(options.format, OutputFormat::Cjs) {
    let are_modules_all_strict = this.modules.iter().all(|id| {
      let is_esm = matches!(graph.module_table.normal_modules[*id].exports_kind, ExportsKind::Esm);
      if is_esm {
        true
      } else {
        graph.ast_table[*id].contains_use_strict
      }
    });

    if are_modules_all_strict {
      concat_source.add_prepend_source(Box::new(RawSource::new("\"use strict\";\n".to_string())));
    }
  }

  if let ChunkKind::EntryPoint { module: entry_id, .. } = this.kind {
    // let entry = &graph.module_table.normal_modules[entry_id];
    let entry_meta = &graph.metas[entry_id];
    match options.format {
      OutputFormat::Esm => match entry_meta.wrap_kind {
        WrapKind::Esm => {
          // init_xxx()
          let wrapper_ref = entry_meta.wrapper_ref.as_ref().unwrap();
          let wrapper_ref_name =
            graph.symbols.canonical_name_for(*wrapper_ref, &this.canonical_names);
          concat_source.add_source(Box::new(RawSource::new(format!("{wrapper_ref_name}();",))));
        }
        WrapKind::Cjs => {
          // "export default require_xxx();"
          let wrapper_ref = entry_meta.wrapper_ref.as_ref().unwrap();
          let wrapper_ref_name =
            graph.symbols.canonical_name_for(*wrapper_ref, &this.canonical_names);
          concat_source.add_source(Box::new(RawSource::new(format!(
            "export default {wrapper_ref_name}();\n"
          ))));
        }
        WrapKind::None => {}
      },
      OutputFormat::Cjs | OutputFormat::App => {}
    }
  }

  match options.format {
    OutputFormat::Esm | OutputFormat::Cjs => {
      if let Some(exports) = render_chunk_exports(this, &graph.runtime, graph, options) {
        concat_source.add_source(Box::new(RawSource::new(exports)));
      }
    }

    OutputFormat::App => {}
  }

  // add footer
  if let Some(footer) = options.footer.as_ref() {
    if let Some(footer_txt) = footer.call(&rendered_chunk).await? {
      if !footer_txt.is_empty() {
        concat_source.add_source(Box::new(RawSource::new(footer_txt)));
      }
    }
  }

  let (content, mut map) = concat_source.content_and_sourcemap();

  // Here file path is generated by chunk file name template, it maybe including path segments.
  // So here need to read it's parent directory as file_dir.
  let file_path = options.cwd.as_path().join(&options.dir).join(
    this
      .preliminary_filename
      .as_deref()
      .expect("chunk file name should be generated before rendering")
      .as_str(),
  );
  let file_dir = file_path.parent().expect("chunk file name should have a parent");

  if let Some(map) = map.as_mut() {
    let paths =
      map.get_sources().map(|source| source.as_path().relative(file_dir)).collect::<Vec<_>>();
    // Here not normalize the windows path, the rollup `sourcemap_path_transform` options need to original path.
    let sources = paths.iter().map(|x| x.to_string_lossy()).collect::<Vec<_>>();
    map.set_sources(sources.iter().map(std::convert::AsRef::as_ref).collect::<Vec<_>>());
  }

  Ok(ChunkRenderReturn {
    code: content,
    map,
    rendered_chunk,
    augment_chunk_hash: None,
    file_dir: file_dir.to_path_buf(),
    preliminary_filename: this
      .preliminary_filename
      .as_deref()
      .expect("should have preliminary filename")
      .clone(),
  })
}
