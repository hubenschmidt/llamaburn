use std::process::Stdio;
use std::time::{Duration, Instant};

use super::ollama_client::StructuredCodeResponse;
use llamaburn_core::{Language, TestCase};
use tempfile::TempDir;
use thiserror::Error;
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum CodeExecutorError {
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CodeExecutorError>;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub passed: bool,
    pub actual_output: String,
    pub expected_output: String,
    pub execution_time_ms: f64,
    pub error: Option<String>,
}

pub struct CodeExecutor {
    temp_dir: TempDir,
}

impl CodeExecutor {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    pub async fn run_tests(
        &self,
        code: &str,
        language: Language,
        test_cases: &[TestCase],
        timeout_ms: u32,
    ) -> Result<Vec<TestResult>> {
        let mut results = Vec::with_capacity(test_cases.len());

        for test_case in test_cases {
            let result = self
                .run_single_test(code, language, test_case, timeout_ms)
                .await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Run tests using structured output - no parsing needed
    pub async fn run_tests_structured(
        &self,
        structured: &StructuredCodeResponse,
        language: Language,
        test_cases: &[TestCase],
        timeout_ms: u32,
    ) -> Result<Vec<TestResult>> {
        let mut results = Vec::with_capacity(test_cases.len());

        for test_case in test_cases {
            let result = self
                .run_single_test_structured(structured, language, test_case, timeout_ms)
                .await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Run a single test using structured output
    async fn run_single_test_structured(
        &self,
        structured: &StructuredCodeResponse,
        language: Language,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        match language {
            Language::Python => self.run_python_structured(structured, test_case, timeout_ms).await,
            Language::JavaScript => self.run_js_structured(structured, test_case, timeout_ms).await,
            Language::Go => self.run_go_structured(structured, test_case, timeout_ms).await,
            Language::Rust => self.run_rust_structured(structured, test_case, timeout_ms).await,
        }
    }

    async fn run_python_structured(
        &self,
        structured: &StructuredCodeResponse,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('\'', "\\'");

        // Build imports from structured output
        // Handle collections imports specially (defaultdict, Counter, deque, etc.)
        let collections_items = ["defaultdict", "Counter", "deque", "OrderedDict", "ChainMap", "namedtuple"];
        let imports = structured.imports.iter()
            .map(|i| format_python_import(i, &collections_items))
            .collect::<Vec<_>>()
            .join("\n");

        let test_code = format!(
            "{imports}\nimport json\nimport sys\n\n{code}\n\nargs = json.loads('{escaped_input}')\nresult = {func_name}(*args)\nprint(json.dumps(result))",
            imports = imports,
            code = structured.code,
            escaped_input = escaped_input,
            func_name = structured.function_name
        );

        let start = Instant::now();
        let output = self.execute_command("python3", &["-c", &test_code], timeout_ms).await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_js_structured(
        &self,
        structured: &StructuredCodeResponse,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('\'', "\\'");

        let test_code = format!(
            "{code}\n\nconst args = JSON.parse('{escaped_input}');\nconst result = {func_name}(...args);\nconsole.log(JSON.stringify(result));",
            code = structured.code,
            escaped_input = escaped_input,
            func_name = structured.function_name
        );

        let start = Instant::now();
        let output = self.execute_command("node", &["-e", &test_code], timeout_ms).await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_go_structured(
        &self,
        structured: &StructuredCodeResponse,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let source_path = self.temp_dir.path().join("main.go");
        let escaped_input = test_case.input.replace('`', "'").replace('"', "\\\"");

        // Build imports from structured output (filter out builtins and unused)
        let builtin = ["encoding/json", "fmt", "reflect"];
        let user_imports = structured.imports.iter()
            .filter(|i| !builtin.contains(&i.as_str()))
            .filter(|i| {
                // Only include if package name appears in code
                let pkg_name = i.rsplit('/').next().unwrap_or(i);
                structured.code.contains(&format!("{}.", pkg_name))
            })
            .map(|i| format!("    \"{}\"", i))
            .collect::<Vec<_>>()
            .join("\n");

        let full_code = format!(
            r#"package main

import (
    "encoding/json"
    "fmt"
    "reflect"
{user_imports}
)

{code}

func main() {{
    var args []interface{{}}
    json.Unmarshal([]byte("{escaped_input}"), &args)

    fn := reflect.ValueOf({func_name})
    fnType := fn.Type()
    callArgs := make([]reflect.Value, len(args))

    for i, arg := range args {{
        callArgs[i] = convertArg(arg, fnType.In(i))
    }}

    results := fn.Call(callArgs)
    if len(results) > 0 {{
        result := results[0].Interface()
        // Handle []byte specially - output as string, not base64
        if b, ok := result.([]byte); ok {{
            fmt.Printf("%q\n", string(b))
        }} else {{
            output, _ := json.Marshal(result)
            fmt.Println(string(output))
        }}
    }}
}}

func convertArg(arg interface{{}}, targetType reflect.Type) reflect.Value {{
    switch targetType.Kind() {{
    case reflect.Slice:
        if s, ok := arg.(string); ok && targetType.Elem().Kind() == reflect.Uint8 {{
            return reflect.ValueOf([]byte(s))
        }}
        arr, ok := arg.([]interface{{}})
        if !ok {{
            return reflect.Zero(targetType)
        }}
        slice := reflect.MakeSlice(targetType, len(arr), len(arr))
        for i, v := range arr {{
            slice.Index(i).Set(convertArg(v, targetType.Elem()))
        }}
        return slice
    case reflect.Int, reflect.Int32, reflect.Int64:
        if f, ok := arg.(float64); ok {{
            return reflect.ValueOf(int(f)).Convert(targetType)
        }}
    case reflect.Float32, reflect.Float64:
        if f, ok := arg.(float64); ok {{
            return reflect.ValueOf(f).Convert(targetType)
        }}
    case reflect.String:
        if s, ok := arg.(string); ok {{
            return reflect.ValueOf(s)
        }}
    case reflect.Bool:
        if b, ok := arg.(bool); ok {{
            return reflect.ValueOf(b)
        }}
    }}
    return reflect.ValueOf(arg)
}}
"#,
            user_imports = user_imports,
            code = structured.code,
            escaped_input = escaped_input,
            func_name = structured.function_name,
        );

        fs::write(&source_path, &full_code).await?;

        let start = Instant::now();
        let source_str = source_path.to_str().expect("temp path not UTF-8");
        let output = self.execute_command("go", &["run", source_str], timeout_ms).await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_rust_structured(
        &self,
        structured: &StructuredCodeResponse,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let source_path = self.temp_dir.path().join("solution.rs");
        let binary_path = self.temp_dir.path().join("solution");
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('"', "\\\"");

        // Strip use statements from LLM code (we provide our own to avoid duplicates)
        let clean_code = structured.code.lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.starts_with("use std::collections")
                    && !trimmed.starts_with("use std::cmp")
                    && !trimmed.starts_with("use std::iter")
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Count arguments in input JSON array to generate dynamic call
        let arg_count = count_json_args(&test_case.input);

        // Generate argument declarations (mutable to support &mut refs)
        let arg_decls = (0..arg_count)
            .map(|i| format!("    let mut _arg{} = parse_arg(&args[{}]);", i, i))
            .collect::<Vec<_>>()
            .join("\n");

        // Generate argument references for function call (use as_mut_arg for all)
        let arg_refs = (0..arg_count)
            .map(|i| format!("        _arg{}.as_mut_arg()", i))
            .collect::<Vec<_>>()
            .join(",\n");

        let full_code = format!(
            r##"#![allow(unused)]
use std::collections::{{HashMap, HashSet, BTreeMap, BTreeSet, VecDeque}};
use std::cmp::{{min, max, Ordering}};

{code}

fn main() {{
    let args = parse_json_array("{escaped_input}");
{arg_decls}
    let result = {func_name}(
{arg_refs}
    );
    print_result(&result);
}}

fn parse_json_array(s: &str) -> Vec<String> {{
    let s = s.trim();
    if s.len() < 2 {{ return vec![s.to_string()]; }}
    let inner = &s[1..s.len()-1];
    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    let mut in_string = false;
    let mut prev_char = ' ';
    for c in inner.chars() {{
        if c == '"' && prev_char != '\\' {{ in_string = !in_string; }}
        if !in_string {{
            match c {{
                '[' | '{{' => {{ depth += 1; current.push(c); }}
                ']' | '}}' => {{ depth -= 1; current.push(c); }}
                ',' if depth == 0 => {{
                    result.push(current.trim().to_string());
                    current = String::new();
                    prev_char = c;
                    continue;
                }}
                _ => current.push(c),
            }}
        }} else {{
            current.push(c);
        }}
        prev_char = c;
    }}
    if !current.trim().is_empty() {{
        result.push(current.trim().to_string());
    }}
    result
}}

// Wrapper that stores parsed value and provides conversions
struct Arg {{
    raw: String,
    parsed_str: String,
    parsed_chars: Vec<char>,
    parsed_ints: Vec<i32>,
    parsed_2d_ints: Vec<Vec<i32>>,
}}

fn parse_arg(s: &str) -> Arg {{
    let raw = s.to_string();
    let s = s.trim();
    // Pre-parse string value (strip quotes)
    let parsed_str = if s.starts_with('"') && s.ends_with('"') {{
        s[1..s.len()-1].to_string()
    }} else {{
        s.to_string()
    }};
    // Pre-parse char array
    let parsed_chars = if s.starts_with('[') && s.len() > 2 {{
        let inner = &s[1..s.len()-1];
        inner.split(',')
            .filter_map(|x| x.trim().trim_matches('"').chars().next())
            .collect()
    }} else {{
        vec![]
    }};
    // Pre-parse int array
    let parsed_ints = if s.starts_with('[') && s.len() >= 2 {{
        let inner = &s[1..s.len()-1];
        if inner.trim().is_empty() {{ vec![] }}
        else {{ inner.split(',').filter_map(|x| x.trim().parse().ok()).collect() }}
    }} else {{
        vec![]
    }};
    // Pre-parse 2D int array
    let parsed_2d_ints = if s.starts_with("[[") {{
        parse_json_array(s).into_iter()
            .map(|x| {{
                let x = x.trim();
                if x == "[]" || !x.starts_with('[') {{ return vec![]; }}
                let inner = &x[1..x.len()-1];
                inner.split(',').filter_map(|n| n.trim().parse().ok()).collect()
            }})
            .collect()
    }} else {{
        vec![]
    }};
    Arg {{ raw, parsed_str, parsed_chars, parsed_ints, parsed_2d_ints }}
}}

impl Arg {{
    fn as_mut_arg<'a, T: FromArgMut<'a>>(&'a mut self) -> T {{
        T::from_arg_mut(self)
    }}
}}

// Trait for converting Arg to target types (supports &mut via &mut self)
trait FromArgMut<'a> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self;
}}

impl<'a> FromArgMut<'a> for i32 {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.raw.trim().parse().unwrap_or(0) }}
}}

impl<'a> FromArgMut<'a> for i64 {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.raw.trim().parse().unwrap_or(0) }}
}}

impl<'a> FromArgMut<'a> for usize {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.raw.trim().parse().unwrap_or(0) }}
}}

impl<'a> FromArgMut<'a> for f64 {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.raw.trim().parse().unwrap_or(0.0) }}
}}

impl<'a> FromArgMut<'a> for bool {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.raw.trim() == "true" }}
}}

impl<'a> FromArgMut<'a> for String {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.parsed_str.clone() }}
}}

impl<'a> FromArgMut<'a> for &'a str {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &arg.parsed_str }}
}}

impl<'a> FromArgMut<'a> for Vec<i32> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{
        let s = arg.raw.trim();
        if s == "[]" || !s.starts_with('[') {{ return vec![]; }}
        let inner = &s[1..s.len()-1];
        inner.split(',').filter_map(|x| x.trim().parse().ok()).collect()
    }}
}}

impl<'a> FromArgMut<'a> for Vec<usize> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{
        let s = arg.raw.trim();
        if s == "[]" || !s.starts_with('[') {{ return vec![]; }}
        let inner = &s[1..s.len()-1];
        inner.split(',').filter_map(|x| x.trim().parse().ok()).collect()
    }}
}}

impl<'a> FromArgMut<'a> for Vec<char> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ arg.parsed_chars.clone() }}
}}

impl<'a> FromArgMut<'a> for &'a [char] {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &arg.parsed_chars }}
}}

impl<'a> FromArgMut<'a> for &'a mut Vec<char> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &mut arg.parsed_chars }}
}}

impl<'a> FromArgMut<'a> for &'a [i32] {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &arg.parsed_ints }}
}}

impl<'a> FromArgMut<'a> for &'a mut Vec<i32> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &mut arg.parsed_ints }}
}}

impl<'a> FromArgMut<'a> for &'a mut Vec<Vec<i32>> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &mut arg.parsed_2d_ints }}
}}

impl<'a> FromArgMut<'a> for &'a [Vec<i32>] {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{ &arg.parsed_2d_ints }}
}}

impl<'a> FromArgMut<'a> for Vec<String> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{
        let s = arg.raw.trim();
        if s == "[]" {{ return vec![]; }}
        parse_json_array(s).into_iter()
            .map(|x| {{
                let x = x.trim();
                if x.starts_with('"') && x.ends_with('"') {{ x[1..x.len()-1].to_string() }}
                else {{ x.to_string() }}
            }})
            .collect()
    }}
}}

impl<'a> FromArgMut<'a> for Vec<Vec<i32>> {{
    fn from_arg_mut(arg: &'a mut Arg) -> Self {{
        let s = arg.raw.trim();
        if s == "[]" {{ return vec![]; }}
        parse_json_array(s).into_iter()
            .map(|x| {{
                let x = x.trim();
                if x == "[]" || !x.starts_with('[') {{ return vec![]; }}
                let inner = &x[1..x.len()-1];
                inner.split(',').filter_map(|n| n.trim().parse().ok()).collect()
            }})
            .collect()
    }}
}}

fn print_result<T: std::fmt::Debug>(result: &T) {{
    let s = format!("{{:?}}", result);
    // Convert to JSON-like format
    let s = s.replace(" ", "").replace("'", "\"");
    println!("{{}}", s);
}}
"##,
            code = clean_code,
            escaped_input = escaped_input,
            func_name = structured.function_name,
            arg_decls = arg_decls,
            arg_refs = arg_refs,
        );

        fs::write(&source_path, &full_code).await?;

        let source_str = source_path.to_str().expect("temp path not UTF-8");
        let binary_str = binary_path.to_str().expect("temp path not UTF-8");
        let compile_output = self
            .execute_command("rustc", &[source_str, "-o", binary_str, "--edition=2021"], 30000)
            .await;

        if let Err(e) = &compile_output {
            return Ok(TestResult {
                passed: false,
                actual_output: String::new(),
                expected_output: test_case.expected.clone(),
                execution_time_ms: 0.0,
                error: Some(format!("Compilation failed: {}", e)),
            });
        }

        let start = Instant::now();
        let output = self.execute_command(binary_str, &[], timeout_ms).await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_single_test(
        &self,
        code: &str,
        language: Language,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        match language {
            Language::Python => self.run_python(code, test_case, timeout_ms).await,
            Language::JavaScript => self.run_javascript(code, test_case, timeout_ms).await,
            Language::Rust => self.run_rust(code, test_case, timeout_ms).await,
            Language::Go => self.run_go(code, test_case, timeout_ms).await,
        }
    }

    async fn run_python(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::Python);
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('\'', "\\'");

        let test_code = format!(
            "{code}\n\nimport json\nimport sys\n\nargs = json.loads('{escaped_input}')\nresult = {func_name}(*args)\nprint(json.dumps(result))",
            code = code,
            escaped_input = escaped_input,
            func_name = func_name
        );

        let start = Instant::now();
        let output = self
            .execute_command("python3", &["-c", &test_code], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_javascript(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::JavaScript);
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('\'', "\\'");

        let test_code = format!(
            "{code}\n\nconst args = JSON.parse('{escaped_input}');\nconst result = {func_name}(...args);\nconsole.log(JSON.stringify(result));",
            code = code,
            escaped_input = escaped_input,
            func_name = func_name
        );

        let start = Instant::now();
        let output = self
            .execute_command("node", &["-e", &test_code], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_rust(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::Rust);
        let source_path = self.temp_dir.path().join("solution.rs");
        let binary_path = self.temp_dir.path().join("solution");
        let escaped_input = test_case.input.replace('\\', "\\\\").replace('"', "\\\"");

        let full_code = format!(
            r##"#![allow(unused)]
use std::collections::HashMap;

{code}

fn main() {{
    // Parse input: {escaped_input}
    let args = parse_json_array("{escaped_input}");
    let result = {func_name}(
        parse_int_vec(&args[0]),
        parse_int(&args[1]),
    );
    print_result(&result);
}}

fn parse_json_array(s: &str) -> Vec<String> {{
    let s = s.trim();
    let inner = &s[1..s.len()-1]; // Remove outer []
    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in inner.chars() {{
        match c {{
            '[' => {{ depth += 1; current.push(c); }}
            ']' => {{ depth -= 1; current.push(c); }}
            ',' if depth == 0 => {{
                result.push(current.trim().to_string());
                current = String::new();
            }}
            _ => current.push(c),
        }}
    }}
    if !current.trim().is_empty() {{
        result.push(current.trim().to_string());
    }}
    result
}}

fn parse_int_vec(s: &str) -> Vec<i32> {{
    let s = s.trim();
    if s == "[]" {{ return vec![]; }}
    let inner = &s[1..s.len()-1];
    inner.split(',').filter_map(|x| x.trim().parse().ok()).collect()
}}

fn parse_int(s: &str) -> i32 {{
    s.trim().parse().unwrap_or(0)
}}

fn print_result<T: std::fmt::Debug>(result: &T) {{
    let s = format!("{{:?}}", result);
    // Convert Rust debug format to JSON-like format
    let s = s.replace(" ", "");
    println!("{{}}", s);
}}
"##,
            code = code,
            escaped_input = escaped_input,
            func_name = func_name,
        );

        fs::write(&source_path, &full_code).await?;

        // Compile
        let source_str = source_path.to_str().expect("temp path not UTF-8");
        let binary_str = binary_path.to_str().expect("temp path not UTF-8");
        let compile_output = self
            .execute_command(
                "rustc",
                &[source_str, "-o", binary_str, "--edition=2021"],
                30000,
            )
            .await;

        if let Err(e) = &compile_output {
            return Ok(TestResult {
                passed: false,
                actual_output: String::new(),
                expected_output: test_case.expected.clone(),
                execution_time_ms: 0.0,
                error: Some(format!("Compilation failed: {}", e)),
            });
        }

        let start = Instant::now();
        let output = self.execute_command(binary_str, &[], timeout_ms).await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn run_go(
        &self,
        code: &str,
        test_case: &TestCase,
        timeout_ms: u32,
    ) -> Result<TestResult> {
        let func_name = extract_function_name(code, Language::Go);
        let source_path = self.temp_dir.path().join("main.go");
        let escaped_input = test_case.input.replace('`', "'");

        // Strip package declaration and imports from LLM code (we provide our own)
        let (user_imports, clean_code) = extract_go_imports(code);

        let full_code = format!(
            r#"package main

import (
    "encoding/json"
    "fmt"
    "reflect"
{user_imports}
)

{clean_code}

func main() {{
    var args []interface{{}}
    json.Unmarshal([]byte("{escaped_input}"), &args)

    // Use reflection to call function with dynamic args
    fn := reflect.ValueOf({func_name})
    fnType := fn.Type()
    callArgs := make([]reflect.Value, len(args))

    for i, arg := range args {{
        callArgs[i] = convertArg(arg, fnType.In(i))
    }}

    results := fn.Call(callArgs)
    if len(results) > 0 {{
        output, _ := json.Marshal(results[0].Interface())
        fmt.Println(string(output))
    }}
}}

func convertArg(arg interface{{}}, targetType reflect.Type) reflect.Value {{
    switch targetType.Kind() {{
    case reflect.Slice:
        // Handle string -> []byte conversion
        if s, ok := arg.(string); ok && targetType.Elem().Kind() == reflect.Uint8 {{
            return reflect.ValueOf([]byte(s))
        }}
        arr, ok := arg.([]interface{{}})
        if !ok {{
            return reflect.Zero(targetType)
        }}
        slice := reflect.MakeSlice(targetType, len(arr), len(arr))
        for i, v := range arr {{
            slice.Index(i).Set(convertArg(v, targetType.Elem()))
        }}
        return slice
    case reflect.Int, reflect.Int32, reflect.Int64:
        if f, ok := arg.(float64); ok {{
            return reflect.ValueOf(int(f)).Convert(targetType)
        }}
    case reflect.Float32, reflect.Float64:
        if f, ok := arg.(float64); ok {{
            return reflect.ValueOf(f).Convert(targetType)
        }}
    case reflect.String:
        if s, ok := arg.(string); ok {{
            return reflect.ValueOf(s)
        }}
    case reflect.Bool:
        if b, ok := arg.(bool); ok {{
            return reflect.ValueOf(b)
        }}
    }}
    return reflect.ValueOf(arg)
}}
"#,
            user_imports = user_imports,
            clean_code = clean_code,
            escaped_input = escaped_input.replace('"', "\\\""),
            func_name = func_name,
        );

        fs::write(&source_path, &full_code).await?;

        let start = Instant::now();
        let source_str = source_path.to_str().expect("temp path not UTF-8");
        let output = self
            .execute_command("go", &["run", source_str], timeout_ms)
            .await;
        let execution_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        Self::build_test_result(output, test_case, execution_time_ms)
    }

    async fn execute_command(
        &self,
        program: &str,
        args: &[&str],
        timeout_ms: u32,
    ) -> std::result::Result<String, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .current_dir(self.temp_dir.path());

        let child = cmd.spawn().map_err(|e| e.to_string())?;

        let timeout = Duration::from_millis(timeout_ms as u64);
        let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

        match result {
            Ok(Ok(output)) => {
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    Err(format!("{}\n{}", stderr, stdout).trim().to_string())
                }
            }
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Err(format!("Timeout after {}ms", timeout_ms)),
        }
    }

    fn build_test_result(
        output: std::result::Result<String, String>,
        test_case: &TestCase,
        execution_time_ms: f64,
    ) -> Result<TestResult> {
        match output {
            Ok(actual) => {
                let passed = normalize_output(&actual) == normalize_output(&test_case.expected);
                Ok(TestResult {
                    passed,
                    actual_output: actual,
                    expected_output: test_case.expected.clone(),
                    execution_time_ms,
                    error: None,
                })
            }
            Err(e) => Ok(TestResult {
                passed: false,
                actual_output: String::new(),
                expected_output: test_case.expected.clone(),
                execution_time_ms,
                error: Some(e),
            }),
        }
    }
}

impl Default for CodeExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create temp directory")
    }
}

fn extract_function_name(code: &str, language: Language) -> String {
    let pattern = match language {
        Language::Python => r"def\s+(\w+)\s*\(",
        Language::JavaScript => r"function\s+(\w+)\s*\(|const\s+(\w+)\s*=",
        Language::Rust => r"fn\s+(\w+)\s*[<(]",
        Language::Go => r"func\s+(\w+)\s*\(",
    };

    // Common helper function names to skip
    let helpers = ["min", "max", "abs", "main", "helper", "swap", "gcd", "lcm"];

    let re = regex::Regex::new(pattern).expect("invalid function name regex");
    let names: Vec<String> = re.captures_iter(code)
        .filter_map(|c| c.get(1).or_else(|| c.get(2)))
        .map(|m| m.as_str().to_string())
        .collect();

    names.into_iter()
        .find(|name| !helpers.contains(&name.as_str()))
        .unwrap_or_else(|| "solution".to_string())
}

fn normalize_output(s: &str) -> String {
    let trimmed = s.trim().replace(' ', "").replace('\n', "").replace('\r', "");

    // Try to parse as a number and normalize (2 == 2.0)
    if let Ok(n) = trimmed.parse::<f64>() {
        let formatted = format!("{:.10}", n);
        return formatted.trim_end_matches('0').trim_end_matches('.').to_string();
    }

    trimmed
}

/// Extract imports from Go code and return (additional_imports, clean_code)
/// Strips `package main`, import statements, and `func main()` blocks
fn extract_go_imports(code: &str) -> (String, String) {
    #[derive(PartialEq)]
    enum State { Normal, InImportBlock, InMainFunc(usize) }

    let mut state = State::Normal;
    let mut imports = Vec::new();
    let mut clean_lines = Vec::new();

    for line in code.lines() {
        let trimmed = line.trim();
        let open = trimmed.matches('{').count();
        let close = trimmed.matches('}').count();

        state = match (&state, trimmed) {
            (State::InMainFunc(depth), _) => {
                let new_depth = depth + open - close;
                if new_depth == 0 { State::Normal } else { State::InMainFunc(new_depth) }
            }
            (_, t) if t.starts_with("func main(") => {
                let depth = open.saturating_sub(close);
                if depth == 0 && open > 0 { State::Normal } else { State::InMainFunc(depth.max(1)) }
            }
            (_, t) if t.starts_with("package ") => State::Normal,
            (_, t) if t.starts_with("import (") => State::InImportBlock,
            (State::InImportBlock, ")") => State::Normal,
            (State::InImportBlock, t) if !t.is_empty() => {
                imports.push(t.trim_matches('"').to_string());
                State::InImportBlock
            }
            (State::InImportBlock, _) => State::InImportBlock,
            (_, t) if t.starts_with("import \"") => {
                imports.push(t.trim_start_matches("import ").trim_matches('"').to_string());
                State::Normal
            }
            _ => {
                clean_lines.push(line);
                State::Normal
            }
        };
    }

    let clean_code = clean_lines.join("\n");

    // Filter out imports we already provide and unused imports
    let builtin = ["encoding/json", "fmt", "reflect"];
    let user_imports: Vec<String> = imports
        .into_iter()
        .filter(|i| !builtin.contains(&i.as_str()))
        .filter(|i| {
            // Only include import if package name appears in code
            let pkg_name = i.rsplit('/').next().unwrap_or(i);
            clean_code.contains(&format!("{}.", pkg_name))
        })
        .map(|i| format!("    \"{}\"", i))
        .collect();

    (user_imports.join("\n"), clean_code)
}

/// Count the number of top-level arguments in a JSON array
/// e.g., "[[1,2,3], 9]" -> 2, "[3]" -> 1, "[\"hello\"]" -> 1
fn count_json_args(input: &str) -> usize {
    let s = input.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return 1;
    }
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return 0;
    }

    let mut count = 1;
    let mut depth = 0;
    let mut in_string = false;
    let mut prev = ' ';

    for c in inner.chars() {
        if c == '"' && prev != '\\' {
            in_string = !in_string;
        }
        if !in_string {
            match c {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                ',' if depth == 0 => count += 1,
                _ => {}
            }
        }
        prev = c;
    }
    count
}

/// Format a Python import statement with proper syntax
/// Handles collections items, dotted imports, and regular imports
fn format_python_import(import: &str, collections_items: &[&str]) -> String {
    // Collections items need "from collections import X"
    if collections_items.contains(&import) {
        return format!("from collections import {}", import);
    }

    // Dotted imports like "collections.defaultdict" -> "from collections import defaultdict"
    if let Some((module, item)) = import.rsplit_once('.') {
        return format!("from {} import {}", module, item);
    }

    // Regular module import
    format!("import {}", import)
}
