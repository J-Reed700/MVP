/// Common English stop words for search term extraction and memory recall.
pub const STOP_WORDS: &[&str] = &[
    "a", "about", "an", "and", "are", "as", "at", "be", "been", "being", "both", "but", "by",
    "can", "could", "did", "do", "does", "during", "each", "either", "every", "few", "for",
    "from", "had", "has", "have", "he", "her", "him", "his", "how", "i", "if", "in", "into", "is",
    "it", "its", "just", "know", "may", "me", "might", "more", "most", "my", "neither", "no",
    "nor", "not", "of", "on", "or", "other", "our", "out", "shall", "she", "should", "so", "some",
    "such", "than", "that", "the", "their", "them", "then", "these", "they", "this", "those",
    "through", "to", "too", "up", "us", "very", "was", "we", "were", "what", "when", "where",
    "which", "who", "whom", "why", "will", "with", "would", "yet", "you", "your",
    // Domain-specific additions
    "above", "after", "all", "any", "before", "below", "between",
];

/// Check if a word is a stop word.
pub fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.binary_search(&word).is_ok()
}
