use colony_expand::ExpansionContext;
use std::io::Write;

pub fn trace_command(context: &ExpansionContext, command: &str, args: &[String]) {
    if !context.shell_options().xtrace {
        return;
    }

    let ps4 = context.get_var_or_env("PS4").unwrap_or_else(|| "+ ".to_string());

    let mut trace_line = ps4;
    trace_line.push_str(command);

    for arg in args {
        trace_line.push(' ');
        if arg.contains(' ') || arg.contains('\t') || arg.contains('\'') || arg.contains('"') {
            trace_line.push('\'');
            trace_line.push_str(&arg.replace('\'', "'\\''"));
            trace_line.push('\'');
        } else if arg.is_empty() {
            trace_line.push_str("''");
        } else {
            trace_line.push_str(arg);
        }
    }

    let _ = writeln!(std::io::stderr(), "{}", trace_line);
}

pub fn trace_simple_command(context: &ExpansionContext, name: &str, args: &[String]) {
    trace_command(context, name, args);
}

pub fn trace_assignment(context: &ExpansionContext, name: &str, value: &str) {
    if !context.shell_options().xtrace {
        return;
    }

    let ps4 = context.get_var_or_env("PS4").unwrap_or_else(|| "+ ".to_string());
    let _ = writeln!(std::io::stderr(), "{}{}={}", ps4, name, value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_command_disabled() {
        let context = ExpansionContext::new();
        trace_command(&context, "echo", &["hello".to_string()]);
    }

    #[test]
    fn test_trace_command_enabled() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().xtrace = true;
        trace_command(&context, "echo", &["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn test_trace_with_spaces() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().xtrace = true;
        trace_command(&context, "echo", &["hello world".to_string()]);
    }

    #[test]
    fn test_trace_with_empty_arg() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().xtrace = true;
        trace_command(&context, "echo", &["".to_string()]);
    }

    #[test]
    fn test_trace_with_custom_ps4() {
        let mut context = ExpansionContext::new();
        context.shell_options_mut().xtrace = true;
        context.set_var("PS4".to_string(), "++ ".to_string());
        trace_command(&context, "echo", &["test".to_string()]);
    }
}
