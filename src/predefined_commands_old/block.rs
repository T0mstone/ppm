pub use super::r#for::handler as for_handler;
#[cfg(feature = "regex")]
pub use super::regex::regex_sub_block as regex_sub_handler;
use crate::invoke::BlockCommandArgs;
use crate::{BlockHandler, Engine};
use std::collections::HashMap;

/// Creates a `HashMap` with all the predefined block commands.
///
/// Their assigned names are:
/// - `for` for [`for_handler`](fn.for_handler.html)
/// - `setvar` for [`setvar_handler`](fn.setvar_handler.html)
/// - `re_sub` for [`regex_sub_handler`](fn.regex_sub_handler.html)
pub fn get_all_handlers() -> HashMap<String, BlockHandler> {
    let mut res = HashMap::new();
    res.insert("for".to_string(), for_handler as _);
    res.insert("setvar".to_string(), setvar_handler as _);
    #[cfg(feature = "regex")]
    res.insert("re_sub".to_string(), regex_sub_handler as _);
    res
}

/// sets a variable
/// - arguments: the name of the variable. The block is the value the variable is set to
/// - calls `engine.process` on both its arguments before evaluating them
pub fn setvar_handler(args: BlockCommandArgs, engine: &mut Engine) -> String {
    let var = args.arg_str;
    let val = args.body;

    let var = engine.process(var, args.issues);
    let val = engine.process(val, args.issues);

    engine.vars.insert(var, val);
    String::new()
}

// todo: macros
