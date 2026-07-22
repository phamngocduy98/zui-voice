/// Stabilizes authoritative full-hypothesis revisions without confidence scores. A token prefix
/// must survive two consecutive hypotheses before it becomes stable; final text preserves the
/// model's exact punctuation and Unicode spacing.
#[derive(Default)]
pub struct SubtitleStabilizer {
    previous: Vec<Token>,
    committed: Vec<Token>,
    finalized: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    leading: String,
    text: String,
}

impl SubtitleStabilizer {
    pub fn revise(&mut self, hypothesis: &str) -> (String, String) {
        let tokens = tokenize(hypothesis);

        // A revision is authoritative. Tokens that had become stable in an earlier hypothesis
        // must disappear if the latest full hypothesis contradicts them.
        self.committed
            .truncate(common_prefix_len(&self.committed, &tokens));

        let stable_len = common_prefix_len(&self.previous, &tokens);
        if stable_len > self.committed.len() {
            self.committed
                .extend(tokens[self.committed.len()..stable_len].iter().cloned());
        }
        self.previous = tokens;
        let committed = render(&self.committed);
        let unstable = render(&self.previous[self.committed.len().min(self.previous.len())..]);
        (committed, unstable)
    }

    pub fn finalize(&mut self, hypothesis: &str) -> String {
        // The final server result is authoritative; do not reconstruct it from whitespace tokens.
        let utterance = hypothesis.trim();
        if !utterance.is_empty() {
            self.finalized = join_without_exact_overlap(&self.finalized, utterance);
        }
        let finalized = self.finalized.clone();
        self.previous.clear();
        self.committed.clear();
        finalized
    }

    pub fn reset(&mut self) {
        self.previous.clear();
        self.committed.clear();
        self.finalized.clear();
    }
}

fn tokenize(value: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut whitespace = String::new();
    let mut word = String::new();

    let flush_word = |tokens: &mut Vec<Token>, whitespace: &mut String, word: &mut String| {
        if !word.is_empty() {
            tokens.push(Token {
                leading: std::mem::take(whitespace),
                text: std::mem::take(word),
            });
        }
    };

    for character in value.chars() {
        if character.is_whitespace() {
            flush_word(&mut tokens, &mut whitespace, &mut word);
            whitespace.push(character);
        } else if is_cjk_joining_character(character) {
            flush_word(&mut tokens, &mut whitespace, &mut word);
            tokens.push(Token {
                leading: std::mem::take(&mut whitespace),
                text: character.to_string(),
            });
        } else {
            word.push(character);
        }
    }
    flush_word(&mut tokens, &mut whitespace, &mut word);
    tokens
}

fn render(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| format!("{}{}", token.leading, token.text))
        .collect()
}

fn common_prefix_len(left: &[Token], right: &[Token]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(a, b)| a.text == b.text)
        .count()
}

fn join_without_exact_overlap(previous: &str, next: &str) -> String {
    if previous.is_empty() {
        return next.to_string();
    }
    if next.is_empty() {
        return previous.to_string();
    }
    if previous.ends_with(next) {
        return previous.to_string();
    }
    if next.starts_with(previous) {
        return next.to_string();
    }
    if next.chars().next().is_some_and(is_closing_punctuation)
        || previous
            .chars()
            .last()
            .is_some_and(is_cjk_joining_character)
        || next.chars().next().is_some_and(is_cjk_joining_character)
    {
        return format!("{previous}{next}");
    }
    format!("{previous} {next}")
}

pub(crate) fn is_cjk_joining_character(value: char) -> bool {
    matches!(
        value as u32,
        0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0xff66..=0xff9f
    )
}

fn is_closing_punctuation(value: char) -> bool {
    matches!(
        value,
        '.' | ',' | '!' | '?' | ':' | ';' | '，' | '。' | '！' | '？' | '：' | '；'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commits_prefix_seen_in_two_revisions() {
        let mut stabilizer = SubtitleStabilizer::default();
        assert_eq!(
            stabilizer.revise("hello world"),
            ("".into(), "hello world".into())
        );
        assert_eq!(
            stabilizer.revise("hello there"),
            ("hello".into(), " there".into())
        );
    }

    #[test]
    fn finalization_preserves_punctuation_and_cjk() {
        let mut stabilizer = SubtitleStabilizer::default();
        assert_eq!(stabilizer.finalize("Hello, world!"), "Hello, world!");
        assert_eq!(
            stabilizer.finalize("你好，世界。"),
            "Hello, world!你好，世界。"
        );
    }

    #[test]
    fn revisions_are_authoritative_not_appended() {
        let mut stabilizer = SubtitleStabilizer::default();
        let _ = stabilizer.revise("the quick brown");
        assert_eq!(
            stabilizer.revise("the quiet fox"),
            ("the".into(), " quiet fox".into())
        );
    }

    #[test]
    fn contradictions_retract_previously_committed_text() {
        let mut stabilizer = SubtitleStabilizer::default();
        let _ = stabilizer.revise("the quick brown");
        assert_eq!(
            stabilizer.revise("the quick brown fox"),
            ("the quick brown".into(), " fox".into())
        );
        assert_eq!(
            stabilizer.revise("the quiet fox"),
            ("the".into(), " quiet fox".into())
        );
    }

    #[test]
    fn cjk_revisions_stabilize_per_character_without_spaces() {
        let mut stabilizer = SubtitleStabilizer::default();
        assert_eq!(
            stabilizer.revise("你好世界"),
            ("".into(), "你好世界".into())
        );
        assert_eq!(
            stabilizer.revise("你好世界。"),
            ("你好世界".into(), "。".into())
        );
    }
}
