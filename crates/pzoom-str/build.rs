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
    ];

    let mut seen = std::collections::HashSet::new();
    for name in &strings {
        if !seen.insert(*name) {
            panic!("duplicate preloaded string: {name}");
        }
    }

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
    writeln!(f, "];")?;

    Ok(())
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
        .replace(' ', "_")
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
