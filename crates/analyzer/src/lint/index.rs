//! Load declarations for checks such as `class C extends Base` missing a parent
//! constructor call. Composer files are inspected through the checker loader.

use crate::{builtins, collect, dependencies::Dependencies, index::Index, project};
use std::path::PathBuf;

use super::Options;

pub(super) fn load(options: &Options, paths: &[PathBuf]) -> Result<(Index, usize), String> {
    let (files, _) = project::parse_files(paths)?;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(phera_parser::PARSER_STACK_BYTES)
            .spawn_scoped(scope, move || {
                let mut index = builtins::load_supported();
                for file in &files {
                    collect::collect(&mut index, &file.path, &file.source, &file.program);
                }
                let mut dependencies = Dependencies::load(&mut index, &options.paths, paths)?;
                let classes = index
                    .classes
                    .values()
                    .map(|class| class.name.clone())
                    .collect();
                dependencies.resolve_classes(&mut index, classes);
                Ok((index, dependencies.vendor_files))
            })
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|_| "Declaration index worker panicked".to_owned())?
    })
}
