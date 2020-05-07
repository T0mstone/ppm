use crate::invoke::{BasicCommandArgs, BlockCommandArgs};
use crate::util::{Span, SplitNotEscapedString};
use crate::{Engine, Issue};
use regex::Regex;
use std::mem::take;

struct RegexArgs {
    pat: String,
    sub: String,
    text: String,
}

impl RegexArgs {
    pub fn from_basic(args: &BasicCommandArgs) -> Result<Self, Issue> {
        let mut spl = args
            .arg_str
            .splitn_not_escaped(3, ':', '\\', false)
            .into_iter();

        let pat = spl.next().unwrap();
        if pat.is_empty() {
            return Err(args.missing_args("empty regular expressions are not supported"));
        }

        let sub = spl
            .next()
            .ok_or(args.invalid_args("no substitution pattern given".to_string()))?;

        let text = spl
            .next()
            .ok_or(args.invalid_args("no string to substitute to".to_string()))?;
        Ok(Self { pat, sub, text })
    }

    pub fn from_block(args: &mut BlockCommandArgs) -> Result<Self, Issue> {
        let mut spl = args
            .arg_str
            .splitn_not_escaped(2, ':', '\\', false)
            .into_iter();

        let pat = spl.next().unwrap();
        if pat.is_empty() {
            return Err(args.missing_args("empty regular expressions are not supported"));
        }

        let sub = spl
            .next()
            .ok_or(args.invalid_args("no substitution pattern given".to_string()))?;

        Ok(Self {
            pat,
            sub,
            text: take(&mut args.body),
        })
    }
}

fn regex_impl(
    args: RegexArgs,
    issues: &mut Vec<Issue>,
    start_cmd_span: Span,
    engine: &mut Engine,
) -> String {
    let re = match Regex::new(&args.pat) {
        Ok(x) => x,
        Err(e) => {
            issues.push(Issue {
                id: "command:invalid_args",
                msg: format!("error compiling regex: {}", e),
                span: start_cmd_span,
            });
            return String::new();
        }
    };

    let text = engine.process(args.text, issues);
    let rep = re.replace_all(&text, &args.sub[..]);
    engine.process(rep.to_string(), issues)
}

/// (requires the `regex` feature) handles a regular expression (using the [`regex`](https://docs.rs/regex) crate)
/// - arguments: the regex, the substitution and the text to substitute into, separated with colons
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
/// - calls `engine.process` on its argument before and after substitution
#[inline]
pub fn regex_sub_basic(args: BasicCommandArgs, engine: &mut Engine) -> String {
    let re_args = match RegexArgs::from_basic(&args) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(e);
            return String::new();
        }
    };
    regex_impl(re_args, args.issues, args.cmd_span, engine)
}

/// (requires the `regex` feature) handles a regular expression (using the [`regex`](https://docs.rs/regex) crate)
/// - arguments: the regex and the substitution, separated with a colon. Then, in the body, the text to be substituted into
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
/// - calls `engine.process` on its argument before and after substitution
#[inline]
pub fn regex_sub_block(mut args: BlockCommandArgs, engine: &mut Engine) -> String {
    let re_args = match RegexArgs::from_block(&mut args) {
        Ok(x) => x,
        Err(e) => {
            args.issues.push(e);
            return String::new();
        }
    };
    regex_impl(re_args, args.issues, args.start_cmd_span, engine)
}
