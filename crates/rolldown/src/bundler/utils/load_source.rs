use rolldown_common::ResolvedPath;
use sugar_path::AsPath;

use crate::{bundler::plugin_driver::PluginDriver, error::BatchedErrors, HookLoadArgs};

pub async fn load_source(
  plugin_driver: &PluginDriver,
  resolved_path: &ResolvedPath,
  fs: &dyn rolldown_fs::FileSystem,
) -> Result<String, BatchedErrors> {
  let source =
    if let Some(r) = plugin_driver.load(&HookLoadArgs { id: &resolved_path.path }).await? {
      r.code
    } else if resolved_path.ignored {
      String::new()
    } else {
      fs.read_to_string(resolved_path.path.as_path())?
    };
  Ok(source)
}