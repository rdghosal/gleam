// TODO: move TestCompileOutput to a test helper crate

#[cfg(test)]
mod generated_tests;

use gleam_core::{
    build::{
        package_compiler::Source, ErlangAppCodegenConfiguration, Mode, Origin, Target,
        TargetCodegenConfiguration,
    },
    config::PackageConfig,
    io::Content,
};
use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    path::{Path, PathBuf},
};

pub fn prepare(path: &str) -> String {
    let root = PathBuf::from(path).canonicalize().unwrap();

    let toml = std::fs::read_to_string(root.join("gleam.toml")).unwrap();
    let config: PackageConfig = toml::from_str(&toml).unwrap();

    let mut sources = read_files(&root, Origin::Src);
    sources.extend(read_files(&root, Origin::Test).into_iter());

    let target = match config.target {
        Target::Erlang => TargetCodegenConfiguration::Erlang {
            app_file: Some(ErlangAppCodegenConfiguration {
                include_dev_deps: true,
            }),
        },
        Target::JavaScript => TargetCodegenConfiguration::JavaScript {
            emit_typescript_definitions: config.javascript.typescript_declarations,
        },
    };

    let ids = gleam_core::uid::UniqueIdGenerator::new();
    let mut modules = im::HashMap::new();
    let mut warnings = Vec::new();
    let filesystem = gleam_core::io::memory::InMemoryFileSystem::new();
    let root = PathBuf::from("/");
    let out = PathBuf::from("/out/lib/the_package");
    let lib = PathBuf::from("/out/lib");
    let mut build_journal = HashSet::new();
    let mut compiler = gleam_core::build::PackageCompiler::new(
        &config,
        Mode::Dev,
        &root,
        &out,
        &lib,
        &target,
        ids,
        filesystem.clone(),
        Some(&mut build_journal),
    );
    compiler.write_entrypoint = false;
    compiler.write_metadata = false;
    compiler.compile_beam_bytecode = false;
    compiler.copy_native_files = false;
    compiler.sources = sources;
    let result = compiler.compile(&mut warnings, &mut modules, &mut im::HashMap::new());
    match result {
        Ok(_) => {
            let files = filesystem.into_contents();
            TestCompileOutput { files, warnings }.as_overview_text()
        }
        Err(error) => error.pretty_string(),
    }
}

#[derive(Debug)]
pub struct TestCompileOutput {
    files: HashMap<PathBuf, Content>,
    warnings: Vec<gleam_core::Warning>,
}

impl TestCompileOutput {
    pub fn as_overview_text(&self) -> String {
        let mut buffer = String::new();
        for (path, content) in self.files.iter().sorted_by(|a, b| a.0.cmp(b.0)) {
            buffer.push_str("//// ");
            buffer.push_str(path.to_str().unwrap());
            buffer.push('\n');

            match content {
                _ if path.ends_with("gleam.mjs") || path.ends_with("gleam.d.ts") => {
                    buffer.push_str("<prelude>")
                }
                Content::Text(text) => buffer.push_str(text),
                Content::Binary(data) => write!(buffer, "{:#?}", data).unwrap(),
            };
            buffer.push('\n');
            buffer.push('\n');
        }

        for warning in self.warnings.iter() {
            write!(buffer, "//// Warning\n{:#?}", warning).unwrap();
            buffer.push('\n');
            buffer.push('\n');
        }

        buffer
    }
}

fn read_files(root: &Path, origin: Origin) -> Vec<Source> {
    let prefix = &match origin {
        Origin::Src => "src",
        Origin::Test => "test",
    };
    let path = root.join(prefix);

    if !path.exists() {
        return vec![];
    }

    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.is_dir() {
                return None;
            }
            if path.extension().unwrap() != "gleam" {
                return None;
            }

            let path = path.strip_prefix(root).unwrap().to_path_buf();
            let name = path
                .strip_prefix(prefix)
                .unwrap()
                .with_extension("")
                .to_string_lossy()
                .to_string();
            Some(Source {
                code: std::fs::read_to_string(entry.path()).unwrap(),
                origin,
                path,
                name,
            })
        })
        .collect()
}
