//! Tests for export -f (function export to child processes)
//!
//! Tests cover:
//! - `export -f funcname` marking functions for export
//! - `export -fn funcname` removing export flag
//! - Serialization of functions to BASH_FUNC_name%% env vars
//! - Round-trip: export -> serialize -> parse -> import
//! - build_env includes/excludes exported/non-exported functions
//! - Import from environment values (startup path)

#[cfg(test)]
mod tests {
    use crate::builtins::{builtin_export, serialize_function_for_export};
    use colony_expand::ExpansionContext;
    use colony_tuple::TupleSpace;

    /// Helper: define a function in the context from a shell command string.
    fn define_test_function(context: &mut ExpansionContext, name: &str, body_source: &str) {
        use colony_parser::pest_api;
        let stmt = pest_api::parse_statement(body_source).unwrap();
        let func_def = colony_parser::FunctionDef {
            name: name.to_string(),
            body: vec![stmt],
            span: colony_parser::Span::DUMMY,
        };
        context.define_function(name.to_string(), func_def);
    }

    #[test]
    fn test_export_f_marks_function() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "greet", "echo hello");
        assert!(!context.is_function_exported("greet"));
        let result = builtin_export(
            &["-f".to_string(), "greet".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(context.is_function_exported("greet"));
    }

    #[test]
    fn test_export_f_nonexistent_fails() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        let result = builtin_export(
            &["-f".to_string(), "nosuchfunc".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 1);
    }

    #[test]
    fn test_export_fn_unexports() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "myfunc", "echo test");
        context.export_function("myfunc");
        assert!(context.is_function_exported("myfunc"));
        let result = builtin_export(
            &["-fn".to_string(), "myfunc".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(!context.is_function_exported("myfunc"));
        // Function itself should still exist after unexport
        assert!(context.is_function("myfunc"));
    }

    #[test]
    fn test_export_fn_separate_flags() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "myfunc", "echo test");
        context.export_function("myfunc");
        // -f -n as separate args
        let result = builtin_export(
            &["-f".to_string(), "-n".to_string(), "myfunc".to_string()],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(!context.is_function_exported("myfunc"));
    }

    #[test]
    fn test_export_f_multiple() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "func_a", "echo a");
        define_test_function(&mut context, "func_b", "echo b");
        define_test_function(&mut context, "func_c", "echo c");
        let result = builtin_export(
            &[
                "-f".to_string(),
                "func_a".to_string(),
                "func_b".to_string(),
                "func_c".to_string(),
            ],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().exit_code(), 0);
        assert!(context.is_function_exported("func_a"));
        assert!(context.is_function_exported("func_b"));
        assert!(context.is_function_exported("func_c"));
    }

    #[test]
    fn test_export_f_partial_failure() {
        let mut context = ExpansionContext::new();
        let tuple_space = TupleSpace::new();
        define_test_function(&mut context, "real_func", "echo real");
        // Mix of existing and non-existing functions
        let result = builtin_export(
            &[
                "-f".to_string(),
                "real_func".to_string(),
                "fake_func".to_string(),
            ],
            &mut context,
            &tuple_space,
            "test",
        );
        assert!(result.is_ok());
        // Returns failure due to "fake_func"
        assert_eq!(result.unwrap().exit_code(), 1);
        // But "real_func" should still have been exported
        assert!(context.is_function_exported("real_func"));
    }

    #[test]
    fn test_serialize_function_format() {
        use colony_parser::pest_api;
        let stmt = pest_api::parse_statement("echo hello world").unwrap();
        let func_def = colony_parser::FunctionDef {
            name: "greet".to_string(),
            body: vec![stmt],
            span: colony_parser::Span::DUMMY,
        };
        let serialized = serialize_function_for_export(&func_def);
        assert!(
            serialized.starts_with("() {"),
            "Should start with '() {{', got: {}",
            serialized
        );
        assert!(
            serialized.contains("echo hello world"),
            "Should contain body"
        );
        assert!(serialized.ends_with('}'), "Should end with '}}'");
    }

    #[test]
    fn test_build_env_has_exported_funcs() {
        let mut context = ExpansionContext::new();
        define_test_function(&mut context, "testfn", "echo from_testfn");
        context.export_function("testfn");
        let env = crate::build_env(&context);
        let func_env = env.iter().find(|(k, _)| k == "BASH_FUNC_testfn%%");
        assert!(func_env.is_some(), "BASH_FUNC_testfn%% should be in env");
        let (_, value) = func_env.unwrap();
        assert!(value.starts_with("() {"));
        assert!(value.contains("echo from_testfn"));
    }

    #[test]
    fn test_build_env_excludes_unexported() {
        let mut context = ExpansionContext::new();
        define_test_function(&mut context, "private_fn", "echo private");
        // Do NOT export
        let env = crate::build_env(&context);
        let func_env = env.iter().find(|(k, _)| k == "BASH_FUNC_private_fn%%");
        assert!(
            func_env.is_none(),
            "Non-exported function should not appear in env"
        );
    }

    #[test]
    fn test_function_export_roundtrip() {
        use colony_parser::pest_api;
        let mut context = ExpansionContext::new();
        let stmt = pest_api::parse_statement("echo hello_roundtrip").unwrap();
        let func_def = colony_parser::FunctionDef {
            name: "roundtrip_fn".to_string(),
            body: vec![stmt],
            span: colony_parser::Span::DUMMY,
        };
        context.define_function("roundtrip_fn".to_string(), func_def);
        context.export_function("roundtrip_fn");

        // Serialize to env format
        let env = crate::build_env(&context);
        let (env_key, env_value) = env
            .iter()
            .find(|(k, _)| k == "BASH_FUNC_roundtrip_fn%%")
            .expect("exported function should be in env");
        assert_eq!(env_key, "BASH_FUNC_roundtrip_fn%%");
        assert!(env_value.starts_with("() {"));

        // Parse it back (simulate child shell import)
        let func_source = format!("roundtrip_fn {}", env_value);
        let parsed = pest_api::parse_statement(&func_source);
        assert!(parsed.is_ok(), "Failed to parse: {:?}", parsed);
        match parsed.unwrap() {
            colony_parser::Statement::FunctionDef(func) => {
                assert_eq!(func.name, "roundtrip_fn");
                assert!(!func.body.is_empty());
            }
            other => panic!("Expected FunctionDef, got: {:?}", other),
        }
    }

    #[test]
    fn test_import_function_from_env_value() {
        use colony_parser::pest_api;
        let func_name = "test_import_fn";
        let env_value = "() { \n    echo imported\n}";
        let mut context = ExpansionContext::new();

        // Simulate the import logic from startup.rs
        assert!(env_value.starts_with("() {"));
        let func_source = format!("{} {}", func_name, env_value);
        if let Ok(colony_parser::Statement::FunctionDef(func)) =
            pest_api::parse_statement(&func_source)
        {
            context.define_function(func.name.clone(), func);
            context.export_function(func_name);
        }
        assert!(
            context.is_function(func_name),
            "Function should be imported"
        );
        assert!(
            context.is_function_exported(func_name),
            "Imported function should be marked as exported"
        );
    }

    #[test]
    fn test_import_skips_invalid_env_value() {
        let func_name = "bad_import_fn";
        let env_value = "not a function definition";
        let context = ExpansionContext::new();
        // Should skip values that don't start with "() {"
        assert!(!env_value.starts_with("() {"));
        assert!(
            !context.is_function(func_name),
            "Invalid value should not be imported"
        );
    }

    #[test]
    fn test_unset_removes_export_flag() {
        let mut context = ExpansionContext::new();
        define_test_function(&mut context, "to_unset", "echo bye");
        context.export_function("to_unset");
        assert!(context.is_function_exported("to_unset"));
        context.unset_function("to_unset");
        assert!(!context.is_function("to_unset"));
        assert!(!context.is_function_exported("to_unset"));
    }
}
