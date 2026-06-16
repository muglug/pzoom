use std::env;
use std::fs::File;
use std::io::{Result, Write};
use std::path::Path;

fn main() -> Result<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("interned_strings.rs");
    let mut f = File::create(dest_path)?;

    // Preloaded interned strings. Generated constants in StrId must match this order.
    let strings = vec![
        "",
        "Closure",
        "Traversable",
        "Iterator",
        "IteratorAggregate",
        "Throwable",
        "Exception",
        "Error",
        "stdClass",
        "Generator",
        "Countable",
        "Stringable",
        "self",
        "static",
        "parent",
        "pzoom_indexed_access",
        "__construct",
        "__destruct",
        "__call",
        "__callStatic",
        "__get",
        "__set",
        "__isset",
        "__unset",
        "__sleep",
        "__wakeup",
        "__serialize",
        "__unserialize",
        "__toString",
        "__invoke",
        "__set_state",
        "__clone",
        "__debugInfo",
        "$this",
        "DOMDocument",
        "DateTime",
        "DateTimeImmutable",
        "MessageFormatter",
        "NumberFormatter",
        "ReflectionClass",
        "ReflectionFunction",
        "SimpleXMLElement",
        "SimpleXMLIterator",
        "__PHP_Incomplete_Class",
        "abs",
        "addcslashes",
        "addslashes",
        "array_combine",
        "array_key_exists",
        "array_keys",
        "array_merge",
        "array_pop",
        "array_push",
        "array_reverse",
        "array_shift",
        "array_slice",
        "array_sum",
        "array_unique",
        "array_unshift",
        "arsort",
        "asin",
        "asort",
        "assert",
        "atan2",
        "base64_decode",
        "base64_encode",
        "base_convert",
        "basename",
        "bin2hex",
        "ceil",
        "chop",
        "chr",
        "chunk_split",
        "class_exists",
        "convert_uudecode",
        "convert_uuencode",
        "cos",
        "count",
        "crc32",
        "ctype_alnum",
        "ctype_alpha",
        "ctype_digit",
        "ctype_lower",
        "ctype_punct",
        "ctype_space",
        "ctype_upper",
        "ctype_xdigit",
        "curl_error",
        "date",
        "date_format",
        "debug_backtrace",
        "decbin",
        "dechex",
        "deg2rad",
        "dirname",
        "escapeshellarg",
        "exp",
        "explode",
        "file_get_contents",
        "filter_var",
        "floatval",
        "floor",
        "fmod",
        "function_exists",
        "get_class",
        "get_object_vars",
        "get_parent_class",
        "get_resource_type",
        "gethostname",
        "getrandmax",
        "gettype",
        "gzcompress",
        "gzdecode",
        "gzdeflate",
        "gzinflate",
        "gzuncompress",
        "hash",
        "hash_equals",
        "hash_hmac",
        "hex2bin",
        "hexdec",
        "highlight_string",
        "htmlentities",
        "htmlspecialchars",
        "htmlspecialchars_decode",
        "http_build_query",
        "implode",
        "in_array",
        "inet_ntop",
        "inet_pton",
        "intdiv",
        "interface_exists",
        "intval",
        "ip2long",
        "is_a",
        "is_bool",
        "is_callable",
        "is_finite",
        "is_float",
        "is_infinite",
        "is_int",
        "is_nan",
        "is_null",
        "is_numeric",
        "is_object",
        "is_resource",
        "is_scalar",
        "is_string",
        "is_subclass_of",
        "join",
        "json_decode",
        "json_encode",
        "krsort",
        "ksort",
        "lcfirst",
        "levenshtein",
        "log",
        "long2ip",
        "ltrim",
        "max",
        "mb_detect_encoding",
        "mb_list_encodings",
        "mb_strlen",
        "mb_strtolower",
        "mb_strtoupper",
        "md5",
        "method_exists",
        "microtime",
        "min",
        "mktime",
        "mt_getrandmax",
        "nl2br",
        "number_format",
        "ord",
        "pack",
        "password_hash",
        "pathinfo",
        "pow",
        "preg_filter",
        "preg_grep",
        "preg_match",
        "preg_match_all",
        "preg_quote",
        "preg_replace",
        "preg_split",
        "print_r",
        "printf",
        "quoted_printable_decode",
        "quoted_printable_encode",
        "rad2deg",
        "rand",
        "range",
        "rawurldecode",
        "rawurlencode",
        "realpath",
        "round",
        "rsort",
        "rtrim",
        "serialize",
        "sha1",
        "sin",
        "socket_strerror",
        "sort",
        "sprintf",
        "sqrt",
        "sscanf",
        "str_ireplace",
        "str_pad",
        "str_repeat",
        "str_replace",
        "str_rot13",
        "str_shuffle",
        "str_split",
        "str_word_count",
        "strcasecmp",
        "strchr",
        "strcmp",
        "strcspn",
        "stream_get_meta_data",
        "strip_tags",
        "stripcslashes",
        "stripos",
        "stripslashes",
        "stristr",
        "strlen",
        "strnatcasecmp",
        "strnatcmp",
        "strncmp",
        "strpbrk",
        "strpos",
        "strrchr",
        "strrev",
        "strrpos",
        "strspn",
        "strstr",
        "strtolower",
        "strtotime",
        "strtoupper",
        "strtr",
        "strval",
        "substr",
        "substr_compare",
        "substr_count",
        "substr_replace",
        "tan",
        "trigger_error",
        "trim",
        "ucfirst",
        "ucwords",
        "unpack",
        "urldecode",
        "urlencode",
        "utf8_decode",
        "utf8_encode",
        "var_dump",
        "var_export",
        "version_compare",
        "vsprintf",
        "wordwrap",
        "usort",
        "uasort",
        "uksort",
        "array_map",
        "array_filter",
        "array_find",
        "array_find_key",
        "array_any",
        "array_all",
        "array_replace",
        "class_alias",
        "html_entity_decode",
        "str_getcsv",
        "quotemeta",
        "formatMessage",
        "class-string-map",
        "Attribute",
        "offsetGet",
        "offsetSet",
        "offsetExists",
        "offsetUnset",
        "ArrayAccess",
        "DateTimeInterface",
        "PDO",
        "UnitEnum",
        "BackedEnum",
        "IntBackedEnum",
        "StringBackedEnum",
        "name",
        "value",
        "cases",
        "from",
        "tryFrom",
        "$array",
        "$value",
        "$string",
        "pzoom_value_of",
        // PHP core classlikes (verbatim), so their ids are stable across
        // threaded interners and usable as StrId constants.
        "WeakMap",
        "WeakReference",
        "ArrayObject",
        "ArrayIterator",
        "SplObjectStorage",
        "SplDoublyLinkedList",
        "SplStack",
        "SplQueue",
        "SplFixedArray",
        "SplPriorityQueue",
        "SplHeap",
        "SplMinHeap",
        "SplMaxHeap",
        "SplFileInfo",
        "SplFileObject",
        "JsonSerializable",
        "Serializable",
        "SensitiveParameter",
        "AllowDynamicProperties",
        "Override",
        "Deprecated",
        "ReturnTypeWillChange",
        "UnhandledMatchError",
        "ValueError",
        "TypeError",
        "ArgumentCountError",
        "ArithmeticError",
        "DivisionByZeroError",
        "RuntimeException",
        "LogicException",
        "InvalidArgumentException",
        "DomainException",
        "LengthException",
        "OutOfBoundsException",
        "OutOfRangeException",
        "OverflowException",
        "RangeException",
        "UnderflowException",
        "UnexpectedValueException",
        "BadFunctionCallException",
        "BadMethodCallException",
        "ErrorException",
        "JsonException",
        "DateInterval",
        "DatePeriod",
        "DateTimeZone",
        "PDOStatement",
        "PDOException",
        "CurlHandle",
        "mysqli",
        "SQLite3",
        "DOMXPath",
        "DOMNode",
        "DOMElement",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionNamedType",
        "ReflectionParameter",
        "ReflectionObject",
    ];

    let mut seen = std::collections::HashSet::new();
    for name in &strings {
        if !seen.insert(*name) {
            panic!("duplicate preloaded string: {name}");
        }
    }

    // Pull every class / interface / trait / enum name, plus every function and
    // method name, out of the bundled `.phpstub` files so they receive
    // build-time-constant `StrId`s. These are appended *after* the named
    // constants above (whose ids must stay fixed), so they get ids
    // >= strings.len() and no named constant of their own. Built-in symbols are
    // by far the most-referenced names in real code, so pre-interning them keeps
    // their ids identical regardless of scan order and lets every worker resolve
    // them against the shared parent interner without per-file work.
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let stubs_dir = Path::new(&manifest_dir).join("../../stubs");
    println!("cargo:rerun-if-changed={}", stubs_dir.display());
    let mut stub_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    collect_stub_names(&stubs_dir, &mut stub_names);
    let extra_strings: Vec<String> = stub_names
        .into_iter()
        .filter(|s| !seen.contains(s.as_str()))
        .collect();

    writeln!(f, "impl StrId {{")?;
    for (i, name) in strings.iter().enumerate() {
        let const_name = format_identifier(name);
        writeln!(f, "    pub const {}: StrId = StrId({});", const_name, i)?;
    }
    writeln!(f, "}}")?;

    writeln!(f, "pub const PRELOADED_STRINGS: &[&str] = &[")?;
    for name in &strings {
        writeln!(f, "    \"{}\",", name.replace('\\', "\\\\"))?;
    }
    for name in &extra_strings {
        writeln!(f, "    \"{}\",", name.replace('\\', "\\\\"))?;
    }
    writeln!(f, "];")?;

    Ok(())
}

/// Recursively collect class/function/method names from every `.phpstub` file.
fn collect_stub_names(dir: &Path, out: &mut std::collections::BTreeSet<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_stub_names(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("phpstub")
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            extract_names(&content, out);
        }
    }
}

/// Extract declared type/function/method names from PHP stub source.
///
/// This is a deliberately loose lexical scan (no full PHP parse): it walks
/// identifier tokens and, after a `class`/`interface`/`trait`/`enum`/`function`
/// keyword, records the following name. Class-likes and namespaced functions are
/// recorded fully-qualified (leading `\` stripped) to match how the runtime
/// interns resolved names; method names are recorded bare. Stray matches inside
/// comments or strings are harmless — they just become unused preloaded entries.
fn extract_names(src: &str, out: &mut std::collections::BTreeSet<String>) {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut i = 0;
    let mut ns = String::new();
    while i < n {
        if is_ident_start(chars[i]) {
            let start = i;
            while i < n && is_ident_part(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match word.as_str() {
                "namespace" => {
                    skip_ws(&chars, &mut i);
                    ns = read_qualified(&chars, &mut i);
                }
                "class" | "interface" | "trait" | "enum" => {
                    skip_ws(&chars, &mut i);
                    let name = read_ident(&chars, &mut i);
                    if !name.is_empty() && name != "extends" && name != "implements" {
                        out.insert(qualify(&ns, &name));
                    }
                }
                "function" => {
                    skip_ws(&chars, &mut i);
                    if i < n && chars[i] == '&' {
                        i += 1;
                        skip_ws(&chars, &mut i);
                    }
                    let name = read_ident(&chars, &mut i);
                    if !name.is_empty() {
                        // Bare name covers methods and global-namespace functions;
                        // the qualified form covers namespaced free functions.
                        out.insert(name.clone());
                        if !ns.is_empty() {
                            out.insert(qualify(&ns, &name));
                        }
                    }
                }
                _ => {}
            }
        } else {
            i += 1;
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}

fn is_ident_part(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

fn skip_ws(chars: &[char], i: &mut usize) {
    while *i < chars.len() && chars[*i].is_whitespace() {
        *i += 1;
    }
}

fn read_ident(chars: &[char], i: &mut usize) -> String {
    if *i >= chars.len() || !is_ident_start(chars[*i]) {
        return String::new();
    }
    let start = *i;
    while *i < chars.len() && is_ident_part(chars[*i]) {
        *i += 1;
    }
    chars[start..*i].iter().collect()
}

fn read_qualified(chars: &[char], i: &mut usize) -> String {
    let start = *i;
    while *i < chars.len() && (is_ident_part(chars[*i]) || chars[*i] == '\\') {
        *i += 1;
    }
    chars[start..*i]
        .iter()
        .collect::<String>()
        .trim_matches('\\')
        .to_string()
}

fn qualify(ns: &str, name: &str) -> String {
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{}\\{}", ns, name)
    }
}

fn format_identifier(input: &str) -> String {
    if input.is_empty() {
        return "EMPTY".to_string();
    }

    if input == "$$" {
        return "DOLLAR_DOLLAR".to_string();
    }

    if input == "$this" {
        return "THIS_VAR".to_string();
    }

    if input == "__serialize" {
        return "MAGIC_SERIALIZE".to_string();
    }

    if input.starts_with("$_") {
        return "MAGIC_".to_string() + &input[2..];
    }

    // `$array` -> ARRAY_VAR (matching `$this` -> THIS_VAR above).
    if let Some(rest) = input.strip_prefix('$') {
        return format_identifier(rest) + "_VAR";
    }

    if input.starts_with("__") && input.ends_with("__") {
        return input[2..input.len() - 2].to_string() + "_CONST";
    }

    let mut formatted_input = input.to_string();

    formatted_input = formatted_input
        .trim_start_matches("HH\\")
        .trim_start_matches("__")
        .to_string();

    formatted_input = formatted_input
        .replace('\\', "_")
        .replace(['<', '>'], "")
        .replace([' ', '-'], "_")
        .replace('$', "_");

    let mut result = String::new();
    let mut was_lower = false;

    for (i, ch) in formatted_input.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 && was_lower {
                result.push('_');
            }
            result.extend(ch.to_lowercase());
        } else {
            result.push(ch);
        }

        was_lower = ch.is_lowercase();
    }

    result
        .to_uppercase()
        .replace("STD_CLASS", "STDCLASS")
        .replace("SIMPLE_XMLELEMENT", "SIMPLE_XML_ELEMENT")
}
