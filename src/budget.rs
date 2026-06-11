use std::sync::OnceLock;
use tiktoken_rs::CoreBPE;

fn bpe() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| tiktoken_rs::cl100k_base().expect("cl100k_base bundled with tiktoken-rs"))
}

/// Approximate token count using cl100k_base, with a 10% safety margin to
/// account for the difference between tiktoken and Claude's actual tokenizer.
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let raw = bpe().encode_with_special_tokens(text).len();
    (raw as f32 * 1.1) as usize
}

/// Both title and content appear in the injected context, so both cost tokens.
pub fn count_entry_tokens(title: &str, content: &str) -> usize {
    count_tokens(title) + count_tokens(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_zero_tokens() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn nonempty_text_has_tokens() {
        assert!(count_tokens("hello world, this is a test") > 0);
    }

    #[test]
    fn longer_text_costs_at_least_as_much() {
        let short = count_tokens("hello");
        let long = count_tokens("hello there, this is a much longer sentence with more words");
        assert!(long >= short);
    }

    #[test]
    fn entry_tokens_sum_title_and_content() {
        let title = "golang preferences";
        let content = "use uber/zap and sqlc with pgx v5";
        assert_eq!(
            count_entry_tokens(title, content),
            count_tokens(title) + count_tokens(content)
        );
    }
}
