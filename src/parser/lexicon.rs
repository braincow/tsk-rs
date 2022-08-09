use nom::{bytes::complete::take_while1, IResult, character::{is_space, is_newline, complete::char}, sequence::{preceded, separated_pair}, branch::alt, combinator::{all_consuming, map}};
use thiserror::Error;
use anyhow::{Result, bail};

// https://imfeld.dev/writing/parsing_with_nom
// https://github.com/Geal/nom/blob/main/doc/choosing_a_combinator.md
#[derive(Debug, PartialEq, Eq)]
enum ExpressionPrototype<'a> {
    Description(&'a str),
    Project(&'a str),
    Hashtag(&'a str),
    Metadata {
        key: &'a str,
        value: &'a str
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Expression {
    Description(String),
    Project(String),
    Hashtag(String),
    Metadata {
        key: String,
        value: String
    },
}

impl Expression {
    fn from_prototype(prototype: &ExpressionPrototype) -> Self {
        match prototype {
            ExpressionPrototype::Description(text) => Expression::Description(String::from(*text)),
            ExpressionPrototype::Project(text) => Expression::Project(String::from(*text)),
            ExpressionPrototype::Hashtag(text) => Expression::Hashtag(String::from(*text)),
            ExpressionPrototype::Metadata { key, value } => Expression::Metadata { key: String::from(*key), value: String::from(*value) },
        }
    }
}

fn nonws_char(c: char) -> bool {
    !is_space(c as u8) && !is_newline(c as u8)
}

fn allowed_meta_character(c: char) -> bool {
    nonws_char(c) && c != ':'
}

fn word(input: &str) -> IResult<&str, &str> {
    take_while1(nonws_char)(input)
}

fn meta_word(input: &str) -> IResult<&str, &str> {
    take_while1(allowed_meta_character)(input)
}

fn metadata_pair(input: &str) -> IResult<&str, (&str, &str)> {
    separated_pair(meta_word, char(':'), meta_word)(input)
}

fn hashtag(input: &str) -> IResult<&str, &str> {
    preceded(char('#'), word)(input)
}

fn project(input: &str) -> IResult<&str, &str> {
    preceded(char('@'), word)(input)
}

fn metadata(input: &str) -> IResult<&str, (&str, &str)> {
    preceded(char('%'), metadata_pair)(input)
}

fn directive(input: &str) -> IResult<&str, ExpressionPrototype> {
    alt((
    map(hashtag, ExpressionPrototype::Hashtag),
    map(project, ExpressionPrototype::Project),
    map(metadata, |(key, value) | ExpressionPrototype::Metadata {key, value}),
    ))(input)
}

fn parse_inline(input: &str) -> IResult<&str, Vec<ExpressionPrototype>> {
    let mut output = Vec::with_capacity(4);
    let mut current_input = input;

    while !current_input.is_empty() {
        let mut found_directive = false;
        for (current_index, _) in current_input.char_indices() {
            // println!("{} {}", current_index, current_input);
            match directive(&current_input[current_index..]) {
                Ok((remaining, parsed)) => {
                    // println!("Matched {:?} remaining {}", parsed, remaining);
                    let leading_text = &current_input[0..current_index].trim();
                    if !leading_text.is_empty() {
                        output.push(ExpressionPrototype::Description(leading_text));
                    }
                    output.push(parsed);

                    current_input = remaining;
                    found_directive = true;
                    break;
                }
                Err(nom::Err::Error(_)) => {
                    // None of the parsers matched at the current position, so this character is just part of the text.
                    // The iterator will go to the next character so there's nothing to do here.
                }
                Err(e) => {
                    // On any other error, just return the error.
                    return Err(e);
                }
            }
        }

        if !found_directive {
            // no directives matched so just add the text as is into the Description
            output.push(ExpressionPrototype::Description(current_input.trim()));
            break;
        }
    }

    Ok(("", output))
}

#[derive(Error, Debug, PartialEq)]
pub enum LexiconError {
    #[error("i got confused by the language")]
    ParserError(String),
}

pub fn parse(input: String) -> Result<Vec<Expression>> {
    let parsed = alt((
        all_consuming(parse_inline),
    ))(&input)
    .map(|(_, results)| results);

    match parsed {
        Ok(expressions) => {
            Ok(expressions.iter().map(Expression::from_prototype).collect::<Vec<Expression>>())
        },
        Err(error) => bail!(LexiconError::ParserError(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonws_char_allowed() {
        assert!(nonws_char('a'))
    }

    #[test]
    fn nonws_char_whitespace() {
        assert!(!nonws_char(' '))
    }

    #[test]
    fn allowed_meta_character_hyphen() {
        assert!(allowed_meta_character('-'))
    }

    #[test]
    fn allowed_meta_character_whitespace() {
        assert!(!allowed_meta_character(' '))
    }

    #[test]
    fn word_from_trimmed() {
        assert_eq!(word("word").unwrap(), ("", "word"));
    }

    #[test]
    fn word_from_untrimmed_r() {
        assert_eq!(word("word ").unwrap(), (" ", "word"));
    }

    #[test]
    fn word_from_untrimmed_l() {
        assert!(word(" word").is_err());
    }

    #[test]
    fn metaword_from_allowed() {
        assert_eq!(meta_word("x-meta-word").unwrap(), ("", "x-meta-word"));
    }    

    #[test]
    fn metaword_from_whitespace() {
        assert_eq!(meta_word("x-meta -word").unwrap(), (" -word", "x-meta"));
    }

    #[test]
    fn hashtag_valid() {
        assert_eq!(hashtag("#fubar").unwrap(), ("", "fubar"));
    }

    #[test]
    fn hashtag_broken() {
        assert_eq!(hashtag("#fu bar").unwrap(), (" bar", "fu"));
    }

    #[test]
    fn hashtag_broken_noprefix() {
        assert!(hashtag("asfd").is_err());
    }

    #[test]
    fn metadata_pair_valid() {
        assert_eq!(metadata_pair("x-meta:value").unwrap(), ("", ("x-meta", "value")));
    }

    #[test]
    fn project_valid() {
        assert_eq!(project("@fubar").unwrap(), ("", "fubar"));
    }

    #[test]
    fn metadata_pair_broken() {
        assert!(metadata_pair("x-meta: value").is_err());
    }

    #[test]
    fn parse_full_testcase() {
        let input = "some task description here @project-here #taghere #a-second-tag %x-meta:data %fuu:bar additional text at the end";

        let (leftover, mut meta) = parse_inline(input).unwrap();

        assert_eq!(leftover, "");
        // assert the expressions from Vec
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Description("additional text at the end"));
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Metadata { key: "fuu", value: "bar" });
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Metadata { key: "x-meta", value: "data" });
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Hashtag("a-second-tag"));
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Hashtag("taghere"));
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Project("project-here"));
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Description("some task description here"));
    }

    #[test]
    fn parse_full_testcase_no_expressions() {
        let input = "some task description here without expressions";

        let (leftover, mut meta) = parse_inline(input).unwrap();

        assert_eq!(leftover, "");
        // after pulling single description expresssion out of the vec
        assert_eq!(meta.pop().unwrap(), ExpressionPrototype::Description(input));
        // ... check that the vec is actually now empty
        assert!(meta.is_empty());
    }

}