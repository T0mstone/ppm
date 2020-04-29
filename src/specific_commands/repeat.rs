use crate::block_command::BlockCommandArgs;
use crate::util::SplitUnescString;
use crate::Issue;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RepeatConfig {
    For(Vec<String>),
    Range(i128, i128),
}

impl RepeatConfig {
    pub fn new(args: BlockCommandArgs) -> Result<Self, Issue> {
        let mut spl = args.arg_str.split(' ');
        match spl.next() {
            Some("for") => {
                let arg = spl.collect::<Vec<_>>().join(" ");
                Ok(RepeatConfig::For(arg.split_unescaped(':', '\\', false)))
            }
            Some("from") => {
                let from = match spl.next().map(|s| s.parse::<i128>().map_err(|_| s)) {
                    Some(Ok(x)) => x,
                    Some(Err(s)) => {
                        return Err(Issue {
                            id: "command:invalid_args:repeat:range",
                            msg: format!("invalid starting integer: {}", s),
                            span: args.start_cmd_span,
                        })
                    }
                    None => {
                        return Err(Issue {
                            id: "command:invalid_args:repeat:range",
                            msg: "no starting integer given".to_string(),
                            span: args.start_cmd_span,
                        })
                    }
                };
                match spl.next() {
                    Some("to") => (),
                    Some(s) => {
                        return Err(Issue {
                            id: "command:invalid_args:repeat:range",
                            msg: format!("invalid range end: {}", s),
                            span: args.start_cmd_span,
                        })
                    }
                    None => {
                        return Err(Issue {
                            id: "command:invalid_args:repeat:range",
                            msg: "no range end given".to_string(),
                            span: args.start_cmd_span,
                        })
                    }
                }
                let to = match spl.next().map(|s| s.parse::<i128>().map_err(|_| s)) {
                    Some(Ok(x)) => x,
                    Some(Err(s)) => {
                        return Err(Issue {
                            id: "command:invalid_args:repeat:range",
                            msg: format!("invalid ending integer: {}", s),
                            span: args.start_cmd_span,
                        })
                    }
                    None => {
                        return Err(Issue {
                            id: "command:invalid_args:repeat:range",
                            msg: "no ending integer given".to_string(),
                            span: args.start_cmd_span,
                        })
                    }
                };
                Ok(RepeatConfig::Range(from, to))
            }
            Some(x) => Err(Issue {
                id: "command:invalid_args:repeat",
                msg: format!("unknown repeat kind: {}", x),
                span: args.start_cmd_span,
            }),
            None => Err(Issue {
                id: "command:invalid_args:repeat",
                msg: "no repeat kind given".to_string(),
                span: args.start_cmd_span,
            }),
        }
    }
}
