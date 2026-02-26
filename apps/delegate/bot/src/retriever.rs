use anyhow::Result;
use glob::glob;
use regex::RegexBuilder;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub file: PathBuf,
    pub relative_path: String,
    pub matches: Vec<MatchedLine>,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct MatchedLine {
    pub line_number: usize,
    pub content: String,
    pub context: String,
}

/// Grep-like search over workspace markdown files.
/// Returns ranked results with source file attribution.
pub async fn retrieve(
    workspace: &Path,
    terms: &[String],
    bias_terms: &[String],
    max_results: usize,
    context_lines: usize,
) -> Result<Vec<RetrievalResult>> {
    let pattern = workspace.join("**/*.md");
    let pattern_str = pattern.to_string_lossy().replace('\\', "/");

    let files: Vec<PathBuf> = glob(&pattern_str)?
        .filter_map(|entry| entry.ok())
        .collect();

    // Pre-compile all regexes once (not per-line)
    let mut compiled_terms: Vec<(regex::Regex, bool)> = Vec::new();
    for term in terms.iter() {
        let escaped = regex::escape(term);
        if let Ok(re) = RegexBuilder::new(&escaped).case_insensitive(true).build() {
            compiled_terms.push((re, false));
        }
    }
    // Cap bias terms to avoid regex explosion
    for term in bias_terms.iter().take(10) {
        let escaped = regex::escape(term);
        if let Ok(re) = RegexBuilder::new(&escaped).case_insensitive(true).build() {
            compiled_terms.push((re, true));
        }
    }

    let mut results = Vec::new();

    for file in &files {
        let content = match tokio::fs::read_to_string(file).await {
            Ok(c) => c,
            Err(_) => continue,
        };
        let lines: Vec<&str> = content.lines().collect();
        let relative_path = file
            .strip_prefix(workspace)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");

        let mut matches = Vec::new();
        let mut total_score = 0.0;

        for (i, line) in lines.iter().enumerate() {
            let mut line_score = 0.0;

            for (re, is_bias) in &compiled_terms {
                let match_count = re.find_iter(line).count();
                if match_count > 0 {
                    line_score += match_count as f64 * if *is_bias { 0.5 } else { 1.0 };
                }
            }

            if line_score > 0.0 {
                let context_start = i.saturating_sub(context_lines);
                let context_end = (i + context_lines).min(lines.len().saturating_sub(1));
                let context = lines[context_start..=context_end].join("\n");

                matches.push(MatchedLine {
                    line_number: i + 1,
                    content: line.to_string(),
                    context,
                });
                total_score += line_score;
            }
        }

        if !matches.is_empty() {
            let deduped = deduplicate_matches(matches, context_lines);
            results.push(RetrievalResult {
                file: file.clone(),
                relative_path,
                matches: deduped,
                score: total_score,
            });
        }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(max_results);

    Ok(results)
}

/// Extract relevant content from retrieval results as a formatted string.
/// Respects a token budget by truncating results.
pub fn format_retrieved_content(results: &[RetrievalResult], token_budget: usize) -> String {
    let mut sections = Vec::new();
    let mut estimated_tokens = 0;

    for result in results {
        let header = format!(
            "\n--- {} (relevance: {:.1}) ---\n",
            result.relative_path, result.score
        );
        let content: String = result
            .matches
            .iter()
            .map(|m| m.context.as_str())
            .collect::<Vec<_>>()
            .join("\n...\n");
        let section = format!("{}{}", header, content);
        let section_tokens = (section.len() + 3) / 4;

        if estimated_tokens + section_tokens > token_budget {
            let remaining_budget = token_budget.saturating_sub(estimated_tokens);
            if remaining_budget > 100 {
                let char_limit = remaining_budget * 4;
                let truncated: String = section.chars().take(char_limit).collect();
                sections.push(format!("{}\n[...truncated]", truncated));
            }
            break;
        }

        sections.push(section);
        estimated_tokens += section_tokens;
    }

    sections.join("\n")
}

/// Deduplicate overlapping context windows.
fn deduplicate_matches(matches: Vec<MatchedLine>, context_lines: usize) -> Vec<MatchedLine> {
    if matches.len() <= 1 {
        return matches;
    }

    let mut result = vec![matches[0].clone()];
    for curr in matches.iter().skip(1) {
        let prev = result.last_mut().unwrap();
        if curr.line_number - prev.line_number <= context_lines * 2 {
            let prev_line_count = prev.context.lines().count();
            let curr_lines: Vec<&str> = curr.context.lines().collect();
            if curr_lines.len() > prev_line_count {
                let extra = &curr_lines[prev_line_count..];
                prev.context.push('\n');
                prev.context.push_str(&extra.join("\n"));
            }
        } else {
            result.push(curr.clone());
        }
    }

    result
}
