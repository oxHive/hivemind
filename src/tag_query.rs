use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagExpr {
    Tag(String),
    And(Box<TagExpr>, Box<TagExpr>),
    Or(Box<TagExpr>, Box<TagExpr>),
    Not(Box<TagExpr>),
}

impl TagExpr {
    /// Tags are already lowercased both here (at parse time) and at storage
    /// time (src/store.rs), so direct equality is correct.
    pub fn eval(&self, tags: &[String]) -> bool {
        match self {
            TagExpr::Tag(t) => tags.iter().any(|x| x == t),
            TagExpr::And(a, b) => a.eval(tags) && b.eval(tags),
            TagExpr::Or(a, b) => a.eval(tags) || b.eval(tags),
            TagExpr::Not(a) => !a.eval(tags),
        }
    }

    /// Builds an AND-chain from a flat list of required tags — the AND-only
    /// special case used by memory_search's `tags` param (a plain JSON array
    /// has no way to express OR/NOT, so this is the only combinator it needs).
    pub fn and_all(tags: &[String]) -> Option<TagExpr> {
        let mut iter = tags.iter();
        let first = iter.next()?;
        let mut expr = TagExpr::Tag(first.to_lowercase());
        for t in iter {
            expr = TagExpr::And(Box::new(expr), Box::new(TagExpr::Tag(t.to_lowercase())));
        }
        Some(expr)
    }
}

/// True if `s` looks like an attempted tag expression. Callers use this to
/// decide whether to call `parse` or fall back to normal title/FTS
/// resolution — see the detection rule in the design spec.
pub fn looks_like_tag_expr(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("tag:") || t.starts_with("!tag:") || t.starts_with('(')
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    And,
    Or,
    Not,
    LParen,
    RParen,
    Tag(String),
}

pub fn parse(s: &str) -> Result<TagExpr> {
    let tokens = tokenize(s)?;
    let mut pos = 0;
    let expr = parse_or(&tokens, &mut pos)?;
    if pos != tokens.len() {
        bail!("unexpected trailing input in tag expression: {s:?}");
    }
    Ok(expr)
}

fn tokenize(s: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '&' => {
                chars.next();
                tokens.push(Token::And);
            }
            '|' => {
                chars.next();
                tokens.push(Token::Or);
            }
            '!' => {
                chars.next();
                tokens.push(Token::Not);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            _ => {
                let mut word = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || "&|!()".contains(c) {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                match word.strip_prefix("tag:") {
                    Some(value) if !value.is_empty() => {
                        tokens.push(Token::Tag(value.to_lowercase()));
                    }
                    Some(_) => bail!("empty tag value in tag expression: {s:?}"),
                    None => bail!("expected 'tag:' atom, found {word:?} in tag expression: {s:?}"),
                }
            }
        }
    }
    Ok(tokens)
}

fn parse_or(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    let mut expr = parse_and(tokens, pos)?;
    while matches!(tokens.get(*pos), Some(Token::Or)) {
        *pos += 1;
        let rhs = parse_and(tokens, pos)?;
        expr = TagExpr::Or(Box::new(expr), Box::new(rhs));
    }
    Ok(expr)
}

fn parse_and(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    let mut expr = parse_not(tokens, pos)?;
    while matches!(tokens.get(*pos), Some(Token::And)) {
        *pos += 1;
        let rhs = parse_not(tokens, pos)?;
        expr = TagExpr::And(Box::new(expr), Box::new(rhs));
    }
    Ok(expr)
}

fn parse_not(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    if matches!(tokens.get(*pos), Some(Token::Not)) {
        *pos += 1;
        let inner = parse_not(tokens, pos)?;
        return Ok(TagExpr::Not(Box::new(inner)));
    }
    parse_atom(tokens, pos)
}

fn parse_atom(tokens: &[Token], pos: &mut usize) -> Result<TagExpr> {
    match tokens.get(*pos) {
        Some(Token::Tag(t)) => {
            *pos += 1;
            Ok(TagExpr::Tag(t.clone()))
        }
        Some(Token::LParen) => {
            *pos += 1;
            let expr = parse_or(tokens, pos)?;
            match tokens.get(*pos) {
                Some(Token::RParen) => {
                    *pos += 1;
                    Ok(expr)
                }
                _ => bail!("missing closing paren in tag expression"),
            }
        }
        other => bail!("expected tag atom or '(', found {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_tag_expr_detects_expected_prefixes() {
        assert!(looks_like_tag_expr("tag:project:hivemind"));
        assert!(looks_like_tag_expr("!tag:status:done"));
        assert!(looks_like_tag_expr("(tag:a & tag:b)"));
        assert!(looks_like_tag_expr("  tag:project:hivemind")); // leading whitespace trimmed
        assert!(!looks_like_tag_expr("my exact memory title"));
        assert!(!looks_like_tag_expr("plain fts keywords"));
    }

    #[test]
    fn parses_single_tag() {
        let expr = parse("tag:project:hivemind").unwrap();
        assert_eq!(expr, TagExpr::Tag("project:hivemind".to_string()));
    }

    #[test]
    fn parses_and() {
        let expr = parse("tag:a & tag:b").unwrap();
        assert!(expr.eval(&["a".to_string(), "b".to_string()]));
        assert!(!expr.eval(&["a".to_string()]));
    }

    #[test]
    fn parses_or() {
        let expr = parse("tag:a | tag:b").unwrap();
        assert!(expr.eval(&["a".to_string()]));
        assert!(expr.eval(&["b".to_string()]));
        assert!(!expr.eval(&["c".to_string()]));
    }

    #[test]
    fn parses_not() {
        let expr = parse("!tag:done").unwrap();
        assert!(expr.eval(&["other".to_string()]));
        assert!(!expr.eval(&["done".to_string()]));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // a & b | c  ==  (a & b) | c
        let expr = parse("tag:a & tag:b | tag:c").unwrap();
        // Only c present: (a&b) is false, so result depends on c being true
        assert!(expr.eval(&["c".to_string()]));
        // Only a present: (a&b) false, c absent -> false
        assert!(!expr.eval(&["a".to_string()]));
        // a and b present, c absent: (a&b) true -> true
        assert!(expr.eval(&["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn parens_override_precedence() {
        // a & (b | c)
        let expr = parse("tag:a & (tag:b | tag:c)").unwrap();
        assert!(expr.eval(&["a".to_string(), "c".to_string()]));
        assert!(!expr.eval(&["c".to_string()])); // a missing
    }

    #[test]
    fn tag_values_are_lowercased_on_parse() {
        let expr = parse("tag:Project:HiveMind").unwrap();
        assert_eq!(expr, TagExpr::Tag("project:hivemind".to_string()));
    }

    #[test]
    fn unbalanced_paren_is_an_error() {
        assert!(parse("(tag:a & tag:b").is_err());
        assert!(parse("tag:a)").is_err());
    }

    #[test]
    fn bare_word_without_tag_prefix_is_an_error() {
        assert!(parse("tag:a & oops").is_err());
    }

    #[test]
    fn empty_tag_value_is_an_error() {
        assert!(parse("tag:").is_err());
    }

    #[test]
    fn and_all_builds_and_chain() {
        let expr = TagExpr::and_all(&["a".to_string(), "b".to_string(), "c".to_string()]).unwrap();
        assert!(expr.eval(&["a".to_string(), "b".to_string(), "c".to_string()]));
        assert!(!expr.eval(&["a".to_string(), "b".to_string()]));
    }

    #[test]
    fn and_all_empty_returns_none() {
        assert!(TagExpr::and_all(&[]).is_none());
    }
}
