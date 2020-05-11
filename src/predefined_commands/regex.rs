use crate::util::SplitNotEscapedString;
use crate::{CommandConfig, Issue};
use regex::Regex;

struct RegexArgs {
    pat: String,
    sub: String,
    text: String,
}

impl RegexArgs {
    pub fn new(cfg: &mut CommandConfig) -> Result<Self, Issue> {
        let mut spl = cfg
            .process_body()
            // because it is already processed, we don't need tools::split_args here
            .splitn_not_escaped::<Vec<_>>(3, ':', '\\', false)
            .into_iter();

        let pat = spl.next().unwrap();
        if pat.is_empty() {
            return Err(cfg.missing_args("empty regular expressions are not supported"));
        }

        let sub = spl
            .next()
            .ok_or_else(|| cfg.invalid_args("no substitution pattern given".to_string()))?;

        let text = spl
            .next()
            .ok_or_else(|| cfg.invalid_args("no string to substitute to".to_string()))?;
        Ok(Self { pat, sub, text })
    }
}

fn regex_impl(args: RegexArgs, mut cfg: CommandConfig) -> String {
    let re = match Regex::new(&args.pat) {
        Ok(x) => x,
        Err(e) => {
            cfg.push_invalid_args(format!("error compiling regex: {}", e));
            return String::new();
        }
    };

    let text = cfg.process(args.text);
    let rep = re.replace_all(&text, &args.sub[..]);
    cfg.process(rep.to_string())
}

/// (requires the `regex` feature) handles a regular expression (using the [`regex`](https://docs.rs/regex) crate)
/// - arguments: the regex, the substitution and the text to substitute into, separated with colons
///     - escaping colons with `'\\'` is supported, all other instances of `'\\'` are left unchanged
/// - calls `engine.process` on its argument string before doing anything and also after evaluation
#[inline]
pub fn handler(mut cfg: CommandConfig) -> String {
    let re_args = match RegexArgs::new(&mut cfg) {
        Ok(x) => x,
        Err(e) => {
            cfg.issues.push(e);
            return String::new();
        }
    };
    regex_impl(re_args, cfg)
}
