#![deny(clippy::all)]

#[macro_use]
extern crate napi_derive;

use ignore::{WalkBuilder, WalkState};
use std::sync::{Arc, Mutex};
use napi::{Error, Result, Status};
use std::path::Path;

#[napi(object)]
pub struct GlobOptions {
    pub dir: String,
    pub pattern: String,
    pub max_results: Option<u32>,
}

#[napi]
pub async fn glob_search(options: GlobOptions) -> Result<Vec<String>> {
    let dir = &options.dir;
    let pattern_str = &options.pattern;
    let max = options.max_results.unwrap_or(100) as usize;

    let glob_matcher = match globset::Pattern::new(pattern_str) {
        Ok(p) => p.compile_matcher(),
        Err(e) => return Err(Error::new(Status::InvalidArg, format!("Invalid glob pattern: {}", e))),
    };

    let builder = WalkBuilder::new(dir)
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
        .build_parallel();

    let results = Arc::new(Mutex::new(Vec::new()));
    let error_occurred = Arc::new(Mutex::new(None));

    builder.run(|| {
        let results = Arc::clone(&results);
        let error_occurred = Arc::clone(&error_occurred);
        let matcher = glob_matcher.clone();

        Box::new(move |result| {
            if results.lock().unwrap().len() >= max {
                return WalkState::Quit;
            }

            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() {
                        if matcher.is_match(path) {
                            let mut res = results.lock().unwrap();
                            if res.len() < max {
                                res.push(path.to_string_lossy().to_string());
                            }
                        }
                    }
                    WalkState::Continue
                }
                Err(err) => {
                    *error_occurred.lock().unwrap() = Some(err);
                    WalkState::Continue
                }
            }
        })
    });

    let mut res = results.lock().unwrap().clone();
    res.sort();

    Ok(res)
}

#[napi(object)]
pub struct GrepOptions {
    pub dir: String,
    pub pattern: String,
    pub include_pattern: Option<String>,
    pub max_results: Option<u32>,
}

#[napi]
pub async fn grep_search(options: GrepOptions) -> Result<Vec<String>> {
    let dir = &options.dir;
    let regex_pattern = &options.pattern;
    let max = options.max_results.unwrap_or(100) as usize;

    let matcher = match grep_regex::RegexMatcher::new(regex_pattern) {
        Ok(m) => m,
        Err(e) => return Err(Error::new(Status::InvalidArg, format!("Invalid regex: {}", e))),
    };

    let mut builder = WalkBuilder::new(dir);
    builder.hidden(false).ignore(true).git_ignore(true);

    if let Some(inc) = options.include_pattern {
        let mut ovrb = ignore::overrides::OverrideBuilder::new(dir);
        if let Err(e) = ovrb.add(&inc) {
            return Err(Error::new(Status::InvalidArg, format!("Invalid include pattern: {}", e)));
        }
        match ovrb.build() {
            Ok(ovr) => builder.overrides(ovr),
            Err(e) => return Err(Error::new(Status::InvalidArg, format!("Failed building include override: {}", e))),
        };
    }

    let results = Arc::new(Mutex::new(Vec::new()));

    builder.build_parallel().run(|| {
        let results = Arc::clone(&results);
        let matcher = matcher.clone();

        Box::new(move |result| {
            if results.lock().unwrap().len() >= max {
                return WalkState::Quit;
            }

            if let Ok(entry) = result {
                let path = entry.path();
                if path.is_file() {
                    let mut found = false;
                    let mut searcher = grep_searcher::Searcher::new();

                    let _ = searcher.search_path(
                        &matcher,
                        path,
                        grep_searcher::sinks::UTF8(|line_num, line| {
                            found = true;
                            let mut res = results.lock().unwrap();
                            if res.len() < max {
                                res.push(format!("{}:{}:{}", path.display(), line_num, line.trim_end()));
                            }
                            Ok(true)
                        })
                    );

                    if found && results.lock().unwrap().len() >= max {
                        return WalkState::Quit;
                    }
                }
            }
            WalkState::Continue
        })
    });

    let mut res = results.lock().unwrap().clone();
    res.sort();

    Ok(res)
}

#[napi(object)]
pub struct EditOptions {
    pub absolute_path: String,
    pub old_string: String,
    pub new_string: String,
}

#[napi]
pub async fn apply_file_edit(options: EditOptions) -> Result<String> {
    use tokio::fs;

    let path = Path::new(&options.absolute_path);
    if !path.exists() {
        return Err(Error::new(Status::InvalidArg, format!("File does not exist: {}", options.absolute_path)));
    }

    let mut content = fs::read_to_string(path).await.map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;

    let occurrences = content.matches(&options.old_string).count();

    if occurrences == 0 {
        return Err(Error::new(
            Status::InvalidArg,
            "The specified 'old_string' could not be found in the file. Please verify the file content."
        ));
    }

    if occurrences > 1 {
        return Err(Error::new(
            Status::InvalidArg,
            format!("The specified 'old_string' appears {} times. Please provide more context to uniquely identify the snippet to replace.", occurrences)
        ));
    }

    content = content.replace(&options.old_string, &options.new_string);

    fs::write(path, content).await.map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;

    Ok(format!("Successfully replaced 1 occurrence in {}", options.absolute_path))
}
