use super::*;

fn fields(text: &str) -> FrontMatter {
    parse(text).0.expect("front matter")
}

#[test]
fn plain_scalars_and_every_key() {
    let f = fields(
        "---\nname: deploy\ndescription: Deploy to staging\nargument-hint: <env>\n\
         user-invocable: false\nwhen_to_use: after a merge\n---\nBody\n",
    );
    assert_eq!(f.name.as_deref(), Some("deploy"));
    assert_eq!(f.description.as_deref(), Some("Deploy to staging"));
    assert_eq!(f.argument_hint.as_deref(), Some("<env>"));
    assert_eq!(f.user_invocable, Some(false));
    assert_eq!(f.when_to_use.as_deref(), Some("after a merge"));
}

#[test]
fn omp_spells_the_hint_in_camel_case() {
    let f = fields("---\nargumentHint: <file>\n---\n");
    assert_eq!(f.argument_hint.as_deref(), Some("<file>"));
}

#[test]
fn body_starts_after_the_closing_line_and_is_not_trimmed() {
    let (f, body) = parse("---\nname: a\n---\n\n  indented\n");
    assert!(f.is_some());
    assert_eq!(body, "\n  indented\n");
}

#[test]
fn no_front_matter_means_the_whole_text_is_the_body() {
    for text in ["Just text\n---\nname: a\n---\n", " ---\nname: a\n---\n", ""] {
        assert_eq!(parse(text), (None, text), "{text:?}");
    }
}

#[test]
fn a_block_that_is_never_closed_is_not_front_matter() {
    let text = "---\nname: a\nBody without a closing line\n";
    assert_eq!(parse(text), (None, text));
}

#[test]
fn crlf_files_and_a_byte_order_mark_parse() {
    let (f, body) = parse("\u{feff}---\r\nname: a\r\ndescription: d\r\n---\r\nBody\r\n");
    let f = f.expect("front matter");
    assert_eq!(f.name.as_deref(), Some("a"));
    assert_eq!(f.description.as_deref(), Some("d"));
    assert_eq!(body, "Body\r\n");
}

#[test]
fn quoted_scalars_decode_their_escapes() {
    let f =
        fields("---\nname: 'it''s'\ndescription: \"say \\\"hi\\\"\\tnow \\\\ \"  # note\n---\n");
    assert_eq!(f.name.as_deref(), Some("it's"));
    assert_eq!(f.description.as_deref(), Some("say \"hi\"\tnow \\ "));
}

#[test]
fn a_hash_after_a_space_starts_a_comment_in_a_plain_scalar_only() {
    let f = fields("---\nname: a#b\ndescription: Fix it #later\nargument-hint: '#id'\n---\n");
    assert_eq!(f.name.as_deref(), Some("a#b"));
    assert_eq!(f.description.as_deref(), Some("Fix it"));
    assert_eq!(f.argument_hint.as_deref(), Some("#id"));
}

#[test]
fn a_colon_inside_a_value_is_kept() {
    let f = fields("---\ndescription: Run gh issue view:* then stop\n---\n");
    assert_eq!(
        f.description.as_deref(),
        Some("Run gh issue view:* then stop")
    );
}

#[test]
fn literal_block_keeps_line_breaks() {
    let f = fields("---\ndescription: |\n  first\n    second\n\n  third\nname: x\n---\n");
    assert_eq!(f.description.as_deref(), Some("first\n  second\n\nthird"));
    assert_eq!(f.name.as_deref(), Some("x"));
}

#[test]
fn folded_block_joins_lines_with_spaces_and_blank_lines_with_breaks() {
    let f = fields("---\ndescription: >-\n  one\n  two\n\n  three\n---\n");
    assert_eq!(f.description.as_deref(), Some("one two\nthree"));
}

#[test]
fn a_plain_value_continues_on_indented_lines() {
    let f = fields("---\ndescription: Use when\n  the build fails\n  twice\nname: x\n---\n");
    assert_eq!(
        f.description.as_deref(),
        Some("Use when the build fails twice")
    );
    let next_line = fields("---\ndescription:\n  starts below\n---\n");
    assert_eq!(next_line.description.as_deref(), Some("starts below"));
}

#[test]
fn nested_keys_and_sequences_are_ignored() {
    let f = fields(
        "---\nmetadata:\n  name: inner\n  description: inner\ntools:\n- a\n- b\nname: outer\n---\n",
    );
    assert_eq!(f.name.as_deref(), Some("outer"));
    assert_eq!(f.description, None);
}

#[test]
fn empty_values_and_other_user_invocable_values_are_none() {
    let f = fields("---\nname:\ndescription: ''\nuser-invocable: maybe\n---\n");
    assert_eq!(f, FrontMatter::default());
    assert_eq!(
        fields("---\nuser-invocable: TRUE\n---\n").user_invocable,
        Some(true)
    );
}

#[test]
fn broken_yaml_lists_the_file_with_no_fields_set() {
    for block in [
        "name: ok\nthis line is not a key\n",
        "name: ok\ndescription: \"never closed\n",
        "name: ok\ndescription: 'x' trailing\n",
    ] {
        let text = format!("---\n{block}---\nBody\n");
        let (f, body) = parse(&text);
        assert_eq!(f, Some(FrontMatter::default()), "{block:?}");
        assert_eq!(body, "Body\n");
    }
}

#[test]
fn first_line_skips_blank_lines_and_trims() {
    assert_eq!(
        first_line("\n  \n  Review the diff  \nmore"),
        Some("Review the diff")
    );
    assert_eq!(first_line(" \n\t\n"), None);
}
