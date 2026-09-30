//! `ironlint schema` — print the canonical gate-authoring guide.
//!
//! Embeds `adapters/shared/ironlint-config/SKILL.md` and prints its body (YAML
//! frontmatter stripped) to stdout. Read-only; never loads or trusts a config.

use anyhow::Result;

const GUIDE: &str = include_str!("../../../../adapters/shared/ironlint-config/SKILL.md");

/// Strip a leading `--- ... ---` YAML frontmatter block, returning the body.
/// Returns the input unchanged when there is no frontmatter.
fn strip_frontmatter(s: &str) -> &str {
    let Some(rest) = s.strip_prefix("---\n") else {
        return s;
    };
    match rest.find("\n---\n") {
        Some(idx) => rest[idx + "\n---\n".len()..].trim_start_matches('\n'),
        None => s,
    }
}

pub fn run() -> Result<i32> {
    print!("{}", strip_frontmatter(GUIDE));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_block() {
        let doc = "---\nname: x\ndescription: y\n---\n\n# Body\ntext\n";
        assert_eq!(strip_frontmatter(doc), "# Body\ntext\n");
    }

    #[test]
    fn passes_through_when_no_frontmatter() {
        let doc = "# Body\nno frontmatter\n";
        assert_eq!(strip_frontmatter(doc), doc);
    }

    #[test]
    fn passes_through_on_unterminated_frontmatter() {
        let doc = "---\nname: x\nno closing fence\n";
        assert_eq!(strip_frontmatter(doc), doc);
    }

    #[test]
    fn embedded_guide_has_no_frontmatter_after_strip() {
        // The real guide starts with frontmatter; the printed body must not.
        assert!(!strip_frontmatter(GUIDE).starts_with("---"));
        assert!(strip_frontmatter(GUIDE).contains("version: 1"));
        assert!(strip_frontmatter(GUIDE).contains("IRONLINT_ROOT"));
        assert!(!strip_frontmatter(GUIDE).contains("IRONLINT_TMPFILE"));
        assert!(strip_frontmatter(GUIDE).contains("with stdin closed"));
    }

    #[test]
    fn guide_policy_examples_use_the_shared_validated_model() {
        let mut examples = 0;
        for block in GUIDE.split("```yaml\n").skip(1) {
            let yaml = block.split("```").next().unwrap();
            let policy = ironlint_core::config::parse_v1_str(yaml).unwrap();
            assert_eq!(policy.version(), 1);
            assert!(!policy.checks().is_empty());
            examples += 1;
        }
        assert!(
            examples > 0,
            "the shipped guide must contain a valid policy example"
        );
    }
}
