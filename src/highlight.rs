//! Lightweight syntax highlighter for common file types.
//! Replaces bat for fast, zero-spawn preview highlighting.
//! Supports multiple color themes via Theme struct.

use crust::style;
use std::sync::Mutex;

#[derive(Clone, Copy)]
pub struct Theme {
    pub keyword: u8,
    pub string: u8,
    pub comment: u8,
    pub number: u8,
    pub typ: u8,
    pub func: u8,
    pub preproc: u8,
    pub punct: u8,
}

static ACTIVE_THEME: Mutex<Option<Theme>> = Mutex::new(None);

pub fn set_theme(name: &str) {
    if let Ok(mut t) = ACTIVE_THEME.lock() {
        *t = Some(theme_by_name(name));
    }
}

fn theme() -> Theme {
    ACTIVE_THEME.lock().ok()
        .and_then(|t| *t)
        .unwrap_or_else(|| theme_by_name("monokai"))
}

pub fn theme_by_name(name: &str) -> Theme {
    match name {
        "monokai" => Theme {
            keyword: 197, string: 78, comment: 242, number: 141,
            typ: 81, func: 148, preproc: 197, punct: 248,
        },
        "solarized" => Theme {
            keyword: 136, string: 64, comment: 245, number: 125,
            typ: 33, func: 166, preproc: 136, punct: 240,
        },
        "nord" => Theme {
            keyword: 110, string: 108, comment: 60, number: 176,
            typ: 73, func: 222, preproc: 110, punct: 103,
        },
        "dracula" => Theme {
            keyword: 212, string: 84, comment: 61, number: 141,
            typ: 117, func: 228, preproc: 212, punct: 189,
        },
        "gruvbox" => Theme {
            keyword: 167, string: 142, comment: 245, number: 175,
            typ: 109, func: 214, preproc: 167, punct: 223,
        },
        "plain" => Theme {
            keyword: 252, string: 252, comment: 245, number: 252,
            typ: 252, func: 252, preproc: 252, punct: 245,
        },
        _ => theme_by_name("monokai"),
    }
}

pub fn available_themes() -> &'static [&'static str] {
    &["monokai", "solarized", "nord", "dracula", "gruvbox", "plain"]
}

struct Lang {
    line_comment: &'static [&'static str],
    block_start: &'static str,
    block_end: &'static str,
    keywords: &'static [&'static str],
    types: &'static [&'static str],
}

fn lang_for(ext: &str) -> Option<Lang> {
    match ext {
        "rs" => Some(Lang {
            line_comment: &["//"],
            block_start: "/*", block_end: "*/",
            keywords: &["fn","let","mut","pub","use","mod","struct","enum","impl","trait",
                "for","while","loop","if","else","match","return","break","continue",
                "where","as","in","ref","self","Self","super","crate","async","await",
                "move","dyn","type","const","static","unsafe","extern"],
            types: &["i8","i16","i32","i64","i128","u8","u16","u32","u64","u128",
                "f32","f64","bool","char","str","String","Vec","Option","Result",
                "Box","Rc","Arc","HashMap","HashSet","usize","isize"],
        }),
        "py" => Some(Lang {
            line_comment: &["#"],
            block_start: "\"\"\"", block_end: "\"\"\"",
            keywords: &["def","class","if","elif","else","for","while","return","import",
                "from","as","with","try","except","finally","raise","yield","lambda",
                "pass","break","continue","and","or","not","in","is","None","True","False",
                "global","nonlocal","assert","del","async","await"],
            types: &["int","float","str","bool","list","dict","tuple","set","bytes",
                "type","object","Exception"],
        }),
        "rb" | "gemspec" => Some(Lang {
            line_comment: &["#"],
            block_start: "=begin", block_end: "=end",
            keywords: &["def","class","module","if","elsif","else","unless","while","until",
                "for","do","end","return","yield","begin","rescue","ensure","raise",
                "require","require_relative","include","extend","prepend","puts","print","p",
                "attr_accessor","attr_reader","attr_writer","alias","defined?",
                "nil","true","false","self","super","then","when","case","and","or","not",
                "lambda","proc","block_given?","loop","open","each","map","select","reject",
                "freeze","frozen?","dup","clone","respond_to?","send","method_missing"],
            types: &["String","Integer","Float","Array","Hash","Symbol","Proc","IO","File",
                "Dir","Regexp","Range","Struct","Class","Module","Kernel","Object",
                "NilClass","TrueClass","FalseClass","Numeric","Comparable","Enumerable"],
        }),
        "js" | "ts" | "jsx" | "tsx" => Some(Lang {
            line_comment: &["//"],
            block_start: "/*", block_end: "*/",
            keywords: &["function","const","let","var","if","else","for","while","return",
                "class","extends","import","export","from","default","new","this",
                "try","catch","finally","throw","async","await","yield","switch","case",
                "break","continue","typeof","instanceof","delete","void","in","of"],
            types: &["string","number","boolean","any","void","null","undefined","never",
                "object","Array","Promise","Map","Set","Record","Partial"],
        }),
        "go" => Some(Lang {
            line_comment: &["//"],
            block_start: "/*", block_end: "*/",
            keywords: &["func","var","const","type","struct","interface","map","chan",
                "if","else","for","range","switch","case","default","return","break",
                "continue","go","defer","select","package","import","fallthrough"],
            types: &["int","int8","int16","int32","int64","uint","uint8","uint16",
                "uint32","uint64","float32","float64","string","bool","byte","rune",
                "error","nil","true","false","iota"],
        }),
        "c" | "h" | "cpp" | "hpp" | "cc" => Some(Lang {
            line_comment: &["//"],
            block_start: "/*", block_end: "*/",
            keywords: &["if","else","for","while","do","switch","case","default","return",
                "break","continue","goto","typedef","struct","union","enum","sizeof",
                "static","extern","inline","const","volatile","register","auto",
                "class","public","private","protected","virtual","template","namespace",
                "using","throw","try","catch","new","delete","this","nullptr"],
            types: &["int","char","float","double","void","long","short","unsigned",
                "signed","bool","size_t","string","vector","map","set","auto"],
        }),
        "sh" | "bash" | "zsh" | "fish" => Some(Lang {
            line_comment: &["#"],
            block_start: "", block_end: "",
            keywords: &["if","then","else","elif","fi","for","while","do","done","case",
                "esac","in","function","return","exit","local","export","readonly",
                "source","alias","unset","shift","set","eval","exec","trap","true","false"],
            types: &[],
        }),
        "lua" => Some(Lang {
            line_comment: &["--"],
            block_start: "--[[", block_end: "]]",
            keywords: &["function","local","if","then","else","elseif","end","for","while",
                "do","repeat","until","return","break","in","and","or","not",
                "nil","true","false","require"],
            types: &["string","number","table","boolean","thread","userdata"],
        }),
        "java" | "kt" | "kts" | "scala" => Some(Lang {
            line_comment: &["//"],
            block_start: "/*", block_end: "*/",
            keywords: &["class","interface","extends","implements","import","package",
                "public","private","protected","static","final","abstract","void",
                "new","return","if","else","for","while","do","switch","case","break",
                "continue","try","catch","finally","throw","throws","this","super",
                "null","true","false","instanceof","synchronized","volatile"],
            types: &["int","long","float","double","boolean","char","byte","short",
                "String","Integer","Long","Float","Double","Object","List","Map","Set"],
        }),
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" => Some(Lang {
            line_comment: &["#"],
            block_start: "", block_end: "",
            keywords: &["true","false","yes","no","null","none","on","off"],
            types: &[],
        }),
        "sql" => Some(Lang {
            line_comment: &["--"],
            block_start: "/*", block_end: "*/",
            keywords: &["SELECT","FROM","WHERE","INSERT","UPDATE","DELETE","CREATE","DROP",
                "ALTER","TABLE","INDEX","VIEW","JOIN","LEFT","RIGHT","INNER","OUTER",
                "ON","AND","OR","NOT","IN","IS","NULL","AS","ORDER","BY","GROUP",
                "HAVING","LIMIT","OFFSET","UNION","VALUES","SET","INTO","EXISTS",
                "DISTINCT","BETWEEN","LIKE","COUNT","SUM","AVG","MAX","MIN",
                "select","from","where","insert","update","delete","create","drop",
                "alter","table","index","view","join","left","right","inner","outer",
                "on","and","or","not","in","is","null","as","order","by","group",
                "having","limit","offset","union","values","set","into","exists"],
            types: &["INTEGER","TEXT","REAL","BLOB","VARCHAR","BOOLEAN","TIMESTAMP",
                "BIGINT","SMALLINT","SERIAL","UUID"],
        }),
        "css" | "scss" | "less" => Some(Lang {
            line_comment: &["//"],
            block_start: "/*", block_end: "*/",
            keywords: &["import","media","keyframes","font-face","charset","supports",
                "important","none","auto","inherit","initial","unset"],
            types: &[],
        }),
        "html" | "htm" | "xml" | "svg" => Some(Lang {
            line_comment: &[],
            block_start: "<!--", block_end: "-->",
            keywords: &[],
            types: &[],
        }),
        "asm" | "s" => Some(Lang {
            line_comment: &[";"],
            block_start: "", block_end: "",
            keywords: &["section","global","extern","mov","push","pop","call","ret","jmp",
                "je","jne","jz","jnz","jg","jl","jge","jle","cmp","test","add","sub",
                "mul","div","xor","and","or","not","shl","shr","lea","syscall","int",
                "db","dw","dd","dq","resb","resw","resd","resq","equ","times","incbin"],
            types: &["rax","rbx","rcx","rdx","rsi","rdi","rsp","rbp","r8","r9","r10",
                "r11","r12","r13","r14","r15","eax","ebx","ecx","edx","al","bl","cl","dl"],
        }),
        "pl" | "pm" => Some(Lang {
            line_comment: &["#"],
            block_start: "=pod", block_end: "=cut",
            keywords: &["my","our","local","sub","if","elsif","else","unless","while","until",
                "for","foreach","do","last","next","redo","return","die","warn","print",
                "say","use","require","package","BEGIN","END","eval","chomp","chop",
                "push","pop","shift","unshift","splice","grep","map","sort","keys","values",
                "defined","undef","ref","bless","new","open","close","read","write"],
            types: &["STDIN","STDOUT","STDERR","ARGV","ENV","INC"],
        }),
        "xrpn" => Some(Lang {
            line_comment: &["//"],
            block_start: "", block_end: "",
            keywords: &["LBL","GTO","XEQ","RTN","END","PSE","STOP","ISG","DSE",
                "X<>Y","X<0?","X>0?","X=0?","X!=0?","X<=0?","X>=0?","X<Y?","X>Y?",
                "X=Y?","X!=Y?","X<=Y?","X>=Y?","SF","CF","FS?","FC?",
                "STO","RCL","VIEW","AVIEW","PROMPT","INPUT","CLA","CLX",
                "R^","Rv","LASTX","ENTER","SIGN","ABS","IP","FP","RND","MOD",
                "PI","SEED","RAN","COMB","PERM","FACT","GAMMA","GCD",
                "SIN","COS","TAN","ASIN","ACOS","ATAN","SINH","COSH","TANH",
                "LN","LOG","EXP","10^X","SQRT","X^2","Y^X","1/X",
                "BASE","OCT","HEX","DEC","BIN","AND","OR","XOR","NOT","ROTL","ROTR",
                "SIZE","CLRG","CLST","CLR",
                "FIX","SCI","ENG","ALL","WSIZE","BSIGNED","BUNSGN",
                "AOFF","AON","TONE","BEEP"],
            types: &[],
        }),
        "hl" => Some(Lang {
            line_comment: &["#"],
            block_start: "", block_end: "",
            keywords: &["AND","OR","THEN","IF","ELSE","ALSO",
                "EXAMPLE","CONDITION","ENCRYPTION"],
            types: &[],
        }),
        "zig" => Some(Lang {
            line_comment: &["//"],
            block_start: "", block_end: "",
            keywords: &["fn","pub","const","var","if","else","for","while","return",
                "break","continue","switch","struct","enum","union","error","defer",
                "try","catch","comptime","inline","export","extern","test","unreachable"],
            types: &["i8","i16","i32","i64","u8","u16","u32","u64","f16","f32","f64",
                "bool","void","usize","isize","anytype","type"],
        }),
        _ => None,
    }
}

/// Check if we have a language definition for this extension.
pub fn lang_known(ext: &str) -> Option<()> {
    lang_for(ext).map(|_| ())
}

// HyperList color constants (from HyperList TUI 256-color theme)
const HL_RED: u8 = 196;     // Properties, dates, multi-line +, change markup
const HL_GREEN: u8 = 46;    // Qualifiers [...], checkboxes, state/transition, semicolons
const HL_BLUE: u8 = 33;     // Operators (ALL-CAPS:) - lighter for dark bg readability
const HL_MAGENTA: u8 = 165; // References <...>, identifiers, SKIP/END
const HL_CYAN: u8 = 51;     // Parentheses (...), quoted strings "..."
const HL_YELLOW: u8 = 226;  // Substitutions {...}
const HL_ORANGE: u8 = 208;  // Hash tags #tag
const HL_GRAY: u8 = 245;    // Dimmed/truncation

/// HyperList-specific highlighting (colors from HyperList TUI spec)
pub fn highlight_hyperlist(text: &str, max_lines: usize) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    let mut count = 0;

    for line in text.lines() {
        if count >= max_lines {
            result.push_str(&style::fg("\n...", HL_GRAY));
            break;
        }
        if count > 0 { result.push('\n'); }
        count += 1;

        let trimmed = line.trim_start();
        let indent: String = line.chars().take(line.len() - trimmed.len()).collect();

        // Multi-line indicator: + at start of trimmed line
        if trimmed.starts_with('+') {
            result.push_str(&indent);
            result.push_str(&style::fg(trimmed, HL_RED));
            continue;
        }

        // State marker: | at start
        if trimmed.starts_with('|') {
            result.push_str(&indent);
            result.push_str(&style::fg(trimmed, HL_GREEN));
            continue;
        }

        // Transition marker: / at start (but not /italic/)
        if trimmed.starts_with('/') && !trimmed.ends_with('/') {
            result.push_str(&indent);
            result.push_str(&style::fg(trimmed, HL_GREEN));
            continue;
        }

        // Process character by character
        result.push_str(&indent);
        let work: Vec<char> = trimmed.chars().collect();
        let len = work.len();
        let mut i = 0;

        while i < len {
            let ch = work[i];

            // Checkboxes: [X], [O], [-], [ ], [_]
            if ch == '[' && i + 2 < len && work[i + 2] == ']'
                && matches!(work[i + 1], 'X' | 'x' | 'O' | 'o' | '-' | ' ' | '_')
            {
                let s: String = work[i..i+3].iter().collect();
                result.push_str(&style::fg(&s, HL_GREEN));
                i += 3;
                continue;
            }

            // Qualifiers: [...]
            if ch == '[' {
                let start = i;
                i += 1;
                while i < len && work[i] != ']' { i += 1; }
                if i < len { i += 1; }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_GREEN));
                continue;
            }

            // Substitutions: {...}
            if ch == '{' {
                let start = i;
                i += 1;
                while i < len && work[i] != '}' { i += 1; }
                if i < len { i += 1; }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_YELLOW));
                continue;
            }

            // References: <...> or <<...>>
            if ch == '<' && i + 1 < len && (work[i + 1].is_alphabetic() || work[i + 1] == '<') {
                let start = i;
                i += 1;
                if i < len && work[i] == '<' { i += 1; } // <<
                while i < len && work[i] != '>' { i += 1; }
                if i < len { i += 1; } // >
                if i < len && work[i] == '>' { i += 1; } // >>
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_MAGENTA));
                continue;
            }

            // Parentheses (comments): (...)
            if ch == '(' {
                let start = i;
                i += 1;
                let mut depth = 1;
                while i < len && depth > 0 {
                    if work[i] == '(' { depth += 1; }
                    if work[i] == ')' { depth -= 1; }
                    i += 1;
                }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_CYAN));
                continue;
            }

            // Quoted strings: "..."
            if ch == '"' {
                let start = i;
                i += 1;
                while i < len && work[i] != '"' { i += 1; }
                if i < len { i += 1; }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_CYAN));
                continue;
            }

            // Hash tags: #tag
            if ch == '#' && i + 1 < len && work[i + 1].is_alphanumeric() {
                let start = i;
                i += 1;
                while i < len && (work[i].is_alphanumeric() || work[i] == '_' || work[i] == '-') { i += 1; }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_ORANGE));
                continue;
            }

            // Change markup: ##< ##> ##->
            if ch == '#' && i + 1 < len && work[i + 1] == '#' {
                let start = i;
                i += 2;
                while i < len && !work[i].is_whitespace() { i += 1; }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_RED));
                continue;
            }

            // Dates: YYYY-MM-DD with optional time
            if ch.is_ascii_digit() && i + 9 < len
                && work[i+4] == '-' && work[i+7] == '-'
                && work[i+1].is_ascii_digit() && work[i+2].is_ascii_digit() && work[i+3].is_ascii_digit()
            {
                let start = i;
                i += 10;
                // Optional time: space/T + HH:MM or HH.MM
                if i < len && (work[i] == 'T' || work[i] == ' ') {
                    let peek = i + 1;
                    if peek + 1 < len && work[peek].is_ascii_digit() {
                        i += 1;
                        while i < len && (work[i].is_ascii_digit() || work[i] == ':' || work[i] == '.') { i += 1; }
                    }
                }
                let s: String = work[start..i].iter().collect();
                result.push_str(&style::fg(&s, HL_RED));
                continue;
            }

            // Operators: ALL-CAPS word followed by colon-space
            if ch.is_ascii_uppercase() {
                let start = i;
                while i < len && (work[i].is_ascii_uppercase() || work[i] == '_') { i += 1; }
                if i < len && work[i] == ':' {
                    i += 1; // include the colon
                    let s: String = work[start..i].iter().collect();
                    result.push_str(&style::fg(&s, HL_BLUE));
                    continue;
                }
                // Special keywords: SKIP, END (no colon)
                let word: String = work[start..i].iter().collect();
                if matches!(word.as_str(), "SKIP" | "END") {
                    result.push_str(&style::fg(&word, HL_MAGENTA));
                    continue;
                }
                result.push_str(&word);
                continue;
            }

            // Properties: Word followed by colon-space (mixed case)
            if ch.is_alphabetic() {
                let start = i;
                while i < len && (work[i].is_alphanumeric() || work[i] == '_' || work[i] == '-' || work[i] == '.') { i += 1; }
                if i < len && work[i] == ':' && i + 1 < len && work[i + 1] == ' ' {
                    i += 1; // include the colon
                    let s: String = work[start..i].iter().collect();
                    result.push_str(&style::fg(&s, HL_RED));
                    continue;
                }
                let word: String = work[start..i].iter().collect();
                result.push_str(&word);
                continue;
            }

            // Semicolons
            if ch == ';' {
                result.push_str(&style::fg(";", HL_GREEN));
                i += 1;
                continue;
            }

            // Text formatting: *bold*
            if ch == '*' && i + 1 < len && work[i + 1] != ' ' {
                if let Some(end) = work[i+1..].iter().position(|&c| c == '*') {
                    let s: String = work[i..i+end+2].iter().collect();
                    result.push_str(&style::bold(&s));
                    i += end + 2;
                    continue;
                }
            }

            // Text formatting: /italic/
            if ch == '/' && i + 1 < len && work[i + 1] != ' ' && i > 0 {
                if let Some(end) = work[i+1..].iter().position(|&c| c == '/') {
                    let s: String = work[i..i+end+2].iter().collect();
                    result.push_str(&style::italic(&s));
                    i += end + 2;
                    continue;
                }
            }

            // Text formatting: _underline_
            if ch == '_' && i + 1 < len && work[i + 1] != ' ' {
                if let Some(end) = work[i+1..].iter().position(|&c| c == '_') {
                    let s: String = work[i..i+end+2].iter().collect();
                    result.push_str(&style::underline(&s));
                    i += end + 2;
                    continue;
                }
            }

            result.push(ch);
            i += 1;
        }
    }
    result
}

/// Highlight source code. Returns ANSI-colored string.
pub fn highlight(text: &str, ext: &str, max_lines: usize) -> String {
    let lang = match lang_for(ext) {
        Some(l) => l,
        None => return plain_with_limit(text, max_lines),
    };

    let mut result = String::with_capacity(text.len() * 2);
    let mut in_block_comment = false;
    let mut line_count = 0;

    for line in text.lines() {
        if line_count >= max_lines {
            result.push_str(&style::fg("...", theme().comment));
            break;
        }
        if line_count > 0 { result.push('\n'); }
        line_count += 1;

        // Block comment continuation
        if in_block_comment {
            if !lang.block_end.is_empty() {
                if let Some(pos) = line.find(lang.block_end) {
                    result.push_str(&style::fg(&line[..pos + lang.block_end.len()], theme().comment));
                    in_block_comment = false;
                    let rest = &line[pos + lang.block_end.len()..];
                    if !rest.is_empty() {
                        highlight_line(rest, &lang, &mut result);
                    }
                } else {
                    result.push_str(&style::fg(line, theme().comment));
                }
            } else {
                result.push_str(&style::fg(line, theme().comment));
            }
            continue;
        }

        // Check for line comment
        let trimmed = line.trim_start();
        let mut is_line_comment = false;
        for lc in lang.line_comment {
            if trimmed.starts_with(lc) {
                is_line_comment = true;
                break;
            }
        }
        if is_line_comment {
            result.push_str(&style::fg(line, theme().comment));
            continue;
        }

        // Check for preprocessor (#include, #define, etc.)
        if trimmed.starts_with('#') && matches!(ext, "c" | "h" | "cpp" | "hpp" | "cc") {
            result.push_str(&style::fg(line, theme().preproc));
            continue;
        }

        // Check for block comment start
        if !lang.block_start.is_empty() && trimmed.contains(lang.block_start) {
            if let Some(pos) = line.find(lang.block_start) {
                if !lang.block_end.is_empty() {
                    if let Some(end) = line[pos + lang.block_start.len()..].find(lang.block_end) {
                        // Single-line block comment
                        highlight_line(&line[..pos], &lang, &mut result);
                        let comment_end = pos + lang.block_start.len() + end + lang.block_end.len();
                        result.push_str(&style::fg(&line[pos..comment_end], theme().comment));
                        let rest = &line[comment_end..];
                        if !rest.is_empty() {
                            highlight_line(rest, &lang, &mut result);
                        }
                        continue;
                    }
                }
                // Multi-line block comment starts
                highlight_line(&line[..pos], &lang, &mut result);
                result.push_str(&style::fg(&line[pos..], theme().comment));
                in_block_comment = true;
                continue;
            }
        }

        highlight_line(line, &lang, &mut result);
    }

    result
}

fn highlight_line(line: &str, lang: &Lang, out: &mut String) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Strings
        if ch == '"' || ch == '\'' || ch == '`' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < len {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2; // skip escaped char
                } else if chars[i] == quote {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            out.push_str(&style::fg(&s, theme().string));
            continue;
        }

        // Ruby/Perl globals ($var), instance (@var), class (@@var)
        if (ch == '$' || ch == '@') && i + 1 < len && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_' || chars[i + 1] == '@') {
            let start = i;
            i += 1;
            if i < len && chars[i] == '@' { i += 1; } // @@class_var
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push_str(&style::fg(&s, theme().typ));
            continue;
        }

        // Ruby symbols :name
        if ch == ':' && i + 1 < len && chars[i + 1].is_alphabetic() {
            let start = i;
            i += 1;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push_str(&style::fg(&s, theme().string));
            continue;
        }

        // CLI flags: --flag or -f (only after whitespace or start of line)
        if ch == '-' && i + 1 < len && chars[i + 1].is_ascii_alphabetic()
            && (i == 0 || chars[i - 1].is_ascii_whitespace())
        {
            let start = i;
            i += 1;
            if i < len && chars[i] == '-' { i += 1; } // skip second dash
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '_') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push_str(&style::fg(&s, theme().keyword));
            continue;
        }

        // Numbers
        if ch.is_ascii_digit() && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let start = i;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '.' || chars[i] == 'x' || chars[i] == '_') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push_str(&style::fg(&s, theme().number));
            continue;
        }

        // Words (identifiers / keywords)
        if ch.is_alphanumeric() || ch == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if lang.keywords.contains(&word.as_str()) {
                out.push_str(&style::fg(&word, theme().keyword));
            } else if lang.types.contains(&word.as_str()) {
                out.push_str(&style::fg(&word, theme().typ));
            } else if i < len && chars[i] == '(' {
                out.push_str(&style::fg(&word, theme().func));
            } else if word.len() > 1 && word.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                // ALL_CAPS constants
                out.push_str(&style::fg(&word, theme().typ));
            } else {
                out.push_str(&word);
            }
            continue;
        }

        // Punctuation
        if matches!(ch, '{' | '}' | '(' | ')' | '[' | ']' | ';' | ':' | ',' | '.' | '-' | '+' | '*' | '/' | '=' | '<' | '>' | '!' | '&' | '|' | '^' | '~' | '%' | '?' | '@') {
            out.push_str(&style::fg(&ch.to_string(), theme().punct));
            i += 1;
            continue;
        }

        out.push(ch);
        i += 1;
    }
}

fn plain_with_limit(text: &str, max_lines: usize) -> String {
    let mut result = String::with_capacity(text.len());
    let mut count = 0;
    for line in text.lines() {
        if count >= max_lines {
            result.push_str(&style::fg("\n...", theme().comment));
            break;
        }
        if count > 0 { result.push('\n'); }
        result.push_str(line);
        count += 1;
    }
    result
}

// Shared inline color helpers for markdown/text highlighters
const MD_H1: u8 = 51;        // bright cyan
const MD_H2: u8 = 117;       // cyan
const MD_H3: u8 = 220;       // yellow
const MD_H_OTHER: u8 = 165;  // magenta
const MD_BOLD: u8 = 255;     // bright white
const MD_CODE: u8 = 78;      // green
const MD_LINK_TEXT: u8 = 81; // bright blue
const MD_LINK_URL: u8 = 245; // dim
const MD_QUOTE: u8 = 245;    // dim
const MD_BULLET: u8 = 220;   // yellow
const MD_RULE: u8 = 240;     // dim
const MD_HTML: u8 = 108;     // muted green for html tags

const TEX_CMD: u8 = 51;      // bright cyan
const TEX_ENV: u8 = 117;     // cyan (bold at callsite)
const TEX_COMMENT: u8 = 245; // dim
const TEX_MATH: u8 = 228;    // bright yellow
const TEX_MATH_DELIM: u8 = 220; // yellow
const TEX_BRACE: u8 = 248;   // light gray
const TEX_OPT: u8 = 176;     // mauve (optional args)

const TXT_URL: u8 = 81;      // bright blue
const TXT_EMAIL: u8 = 78;    // green
const TXT_TODO: u8 = 208;    // orange

/// Markdown highlighter: headers, bold, italic, inline/fenced code, links,
/// blockquotes, lists, horizontal rules.
pub fn highlight_markdown(text: &str, max_lines: usize) -> String {
    // Expand Markdown tables into Unicode-box blocks before per-line
    // highlighting so the highlighter sees already-laid-out rows.
    let text = crust::text::format_markdown_tables(text, 100);
    let mut out = String::with_capacity(text.len() * 2);
    let mut in_fence = false;
    let mut fence_marker = String::new();
    let mut count = 0;

    for line in text.lines() {
        if count >= max_lines {
            out.push_str(&style::fg("\n...", MD_RULE));
            break;
        }
        if count > 0 { out.push('\n'); }
        count += 1;

        let trimmed = line.trim_start();

        // Fenced code block detection
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = &trimmed[..3];
            if in_fence {
                if fence_marker == marker {
                    in_fence = false;
                    fence_marker.clear();
                }
            } else {
                in_fence = true;
                fence_marker = marker.to_string();
            }
            out.push_str(&style::fg(line, MD_CODE));
            continue;
        }
        if in_fence {
            out.push_str(&style::fg(line, MD_CODE));
            continue;
        }

        // Horizontal rule
        let no_ws: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
        if no_ws.len() >= 3
            && (no_ws.chars().all(|c| c == '-')
                || no_ws.chars().all(|c| c == '*')
                || no_ws.chars().all(|c| c == '_'))
        {
            out.push_str(&style::fg(line, MD_RULE));
            continue;
        }

        // Headers
        if let Some(rest) = trimmed.strip_prefix("###### ") {
            out.push_str(&style::bold(&style::fg(&format!("###### {}", rest), MD_H_OTHER)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("##### ") {
            out.push_str(&style::bold(&style::fg(&format!("##### {}", rest), MD_H_OTHER)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#### ") {
            out.push_str(&style::bold(&style::fg(&format!("#### {}", rest), MD_H_OTHER)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push_str(&style::bold(&style::fg(&format!("### {}", rest), MD_H3)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push_str(&style::bold(&style::fg(&format!("## {}", rest), MD_H2)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push_str(&style::bold(&style::fg(&format!("# {}", rest), MD_H1)));
            continue;
        }

        // Blockquote
        if trimmed.starts_with('>') {
            out.push_str(&style::italic(&style::fg(line, MD_QUOTE)));
            continue;
        }

        // Reproduce leading whitespace before styled content
        let lead_ws = &line[..line.len() - trimmed.len()];
        out.push_str(lead_ws);

        // List item marker
        let (marker_end, rest_after_marker) = detect_list_marker(trimmed);
        if marker_end > 0 {
            out.push_str(&style::bold(&style::fg(&trimmed[..marker_end], MD_BULLET)));
            inline_md(rest_after_marker, &mut out);
            continue;
        }

        inline_md(trimmed, &mut out);
    }

    out
}

/// Return (bytes_consumed_by_marker, remainder) if trimmed starts with a list
/// marker ("- ", "* ", "+ ", or "N. "), else (0, trimmed).
fn detect_list_marker(trimmed: &str) -> (usize, &str) {
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 {
        let c = bytes[0];
        if (c == b'-' || c == b'*' || c == b'+') && bytes[1] == b' ' {
            return (2, &trimmed[2..]);
        }
    }
    // Ordered list "123. "
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    if i > 0 && i + 1 < bytes.len() && bytes[i] == b'.' && bytes[i + 1] == b' ' {
        return (i + 2, &trimmed[i + 2..]);
    }
    (0, trimmed)
}

/// Inline markdown: **bold**, *italic* or _italic_, `code`, [text](url),
/// autolinks <url>, HTML tags.
fn inline_md(line: &str, out: &mut String) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Inline code `...`
        if chars[i] == '`' {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                let content: String = chars[i..=i + 1 + end].iter().collect();
                out.push_str(&style::fg(&content, MD_CODE));
                i += 2 + end;
                continue;
            }
        }
        // Bold **...**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            let rest: String = chars[i + 2..].iter().collect();
            if let Some(end) = rest.find("**") {
                let content: String = chars[i + 2..i + 2 + end].iter().collect();
                out.push_str(&style::bold(&style::fg(&content, MD_BOLD)));
                i += 4 + end;
                continue;
            }
        }
        // Italic *...* (single) or _..._
        if chars[i] == '*' || chars[i] == '_' {
            let delim = chars[i];
            if i + 1 < chars.len() && chars[i + 1] != delim && chars[i + 1] != ' ' {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == delim) {
                    let content: String = chars[i + 1..i + 1 + end].iter().collect();
                    if !content.contains('\n') && !content.is_empty() {
                        out.push_str(&style::italic(&content));
                        i += 2 + end;
                        continue;
                    }
                }
            }
        }
        // Markdown link [text](url)
        if chars[i] == '[' {
            if let Some(close_txt) = chars[i + 1..].iter().position(|&c| c == ']') {
                let after = i + 1 + close_txt + 1;
                if after < chars.len() && chars[after] == '(' {
                    if let Some(close_url) = chars[after + 1..].iter().position(|&c| c == ')') {
                        let text: String = chars[i + 1..i + 1 + close_txt].iter().collect();
                        let url: String = chars[after + 1..after + 1 + close_url].iter().collect();
                        out.push_str(&style::underline(&style::fg(&text, MD_LINK_TEXT)));
                        out.push_str(&style::fg(&format!("({})", url), MD_LINK_URL));
                        i = after + 1 + close_url + 1;
                        continue;
                    }
                }
            }
        }
        // Autolink <http://...> or HTML tag
        if chars[i] == '<' {
            if let Some(close) = chars[i + 1..].iter().position(|&c| c == '>') {
                let content: String = chars[i + 1..i + 1 + close].iter().collect();
                let seq: String = chars[i..=i + 1 + close].iter().collect();
                if content.starts_with("http://") || content.starts_with("https://") {
                    out.push_str(&style::underline(&style::fg(&seq, MD_LINK_TEXT)));
                } else {
                    out.push_str(&style::fg(&seq, MD_HTML));
                }
                i += 2 + close;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
}

/// LaTeX/TeX highlighter: commands, environments, comments, math, braces.
pub fn highlight_tex(text: &str, max_lines: usize) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut count = 0;
    let mut in_math_block = false;

    for line in text.lines() {
        if count >= max_lines {
            out.push_str(&style::fg("\n...", TEX_COMMENT));
            break;
        }
        if count > 0 { out.push('\n'); }
        count += 1;

        highlight_tex_line(line, &mut out, &mut in_math_block);
    }
    out
}

fn highlight_tex_line(line: &str, out: &mut String, in_math_block: &mut bool) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Line comment %
        if chars[i] == '%' && (i == 0 || chars[i - 1] != '\\') {
            let rest: String = chars[i..].iter().collect();
            out.push_str(&style::fg(&rest, TEX_COMMENT));
            return;
        }
        // Display math $$...$$
        if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '$' {
            *in_math_block = !*in_math_block;
            out.push_str(&style::fg("$$", TEX_MATH_DELIM));
            i += 2;
            continue;
        }
        // Inline math $...$
        if chars[i] == '$' && (i == 0 || chars[i - 1] != '\\') {
            if let Some(end) = chars[i + 1..].iter().position(|&c| c == '$') {
                let content: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push_str(&style::fg("$", TEX_MATH_DELIM));
                out.push_str(&style::fg(&content, TEX_MATH));
                out.push_str(&style::fg("$", TEX_MATH_DELIM));
                i += 2 + end;
                continue;
            }
        }
        if *in_math_block {
            out.push_str(&style::fg(&chars[i].to_string(), TEX_MATH));
            i += 1;
            continue;
        }
        // Commands \foo, including \begin{env}, \end{env}
        if chars[i] == '\\' && i + 1 < chars.len() {
            let start = i;
            i += 1;
            // \ followed by single non-letter punct is itself a command (e.g. \\, \&, \$)
            if !chars[i].is_ascii_alphabetic() {
                let cmd: String = chars[start..=i].iter().collect();
                out.push_str(&style::fg(&cmd, TEX_CMD));
                i += 1;
                continue;
            }
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '*') {
                i += 1;
            }
            let cmd: String = chars[start..i].iter().collect();
            let is_env = cmd == "\\begin" || cmd == "\\end";
            if is_env {
                out.push_str(&style::bold(&style::fg(&cmd, TEX_ENV)));
                // Consume {env} with env name in bold
                if i < chars.len() && chars[i] == '{' {
                    if let Some(close) = chars[i + 1..].iter().position(|&c| c == '}') {
                        out.push_str(&style::fg("{", TEX_BRACE));
                        let env: String = chars[i + 1..i + 1 + close].iter().collect();
                        out.push_str(&style::bold(&style::fg(&env, TEX_ENV)));
                        out.push_str(&style::fg("}", TEX_BRACE));
                        i = i + 1 + close + 1;
                        continue;
                    }
                }
            } else {
                out.push_str(&style::fg(&cmd, TEX_CMD));
                // Optional args [...]
                if i < chars.len() && chars[i] == '[' {
                    if let Some(close) = chars[i + 1..].iter().position(|&c| c == ']') {
                        let opt: String = chars[i..=i + 1 + close].iter().collect();
                        out.push_str(&style::fg(&opt, TEX_OPT));
                        i = i + 1 + close + 1;
                        continue;
                    }
                }
            }
            continue;
        }
        // Braces
        if chars[i] == '{' || chars[i] == '}' {
            out.push_str(&style::fg(&chars[i].to_string(), TEX_BRACE));
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
}

/// Plain text highlighter: URLs, emails, TODO/FIXME/NOTE markers.
pub fn highlight_text(text: &str, max_lines: usize) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut count = 0;
    for line in text.lines() {
        if count >= max_lines {
            out.push_str(&style::fg("\n...", MD_RULE));
            break;
        }
        if count > 0 { out.push('\n'); }
        count += 1;
        highlight_text_line(line, &mut out);
    }
    out
}

fn highlight_text_line(line: &str, out: &mut String) {
    // Tokenize on whitespace to find URLs/emails, then scan each word for TODO etc.
    let mut last = 0;
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        // Skip whitespace
        while i < len && (bytes[i] as char).is_whitespace() { i += 1; }
        let start = i;
        while i < len && !(bytes[i] as char).is_whitespace() { i += 1; }
        if start == i { break; }
        let word = &line[start..i];
        // Flush prior segment
        out.push_str(&line[last..start]);
        last = i;

        if word.starts_with("http://") || word.starts_with("https://") || word.starts_with("ftp://") {
            // Trim common trailing punctuation from the url portion
            let (url, tail) = split_url_tail(word);
            out.push_str(&style::underline(&style::fg(url, TXT_URL)));
            out.push_str(tail);
            continue;
        }
        if is_email_like(word) {
            out.push_str(&style::fg(word, TXT_EMAIL));
            continue;
        }
        // TODO/FIXME/NOTE/HACK/XXX
        let core = word.trim_end_matches(|c: char| c == ':' || c == ',' || c == '.' || c == '!');
        if matches!(core, "TODO" | "FIXME" | "NOTE" | "HACK" | "XXX" | "BUG" | "WARN") {
            out.push_str(&style::bold(&style::fg(word, TXT_TODO)));
            continue;
        }
        out.push_str(word);
    }
    out.push_str(&line[last..]);
}

fn split_url_tail(s: &str) -> (&str, &str) {
    let cut = s.trim_end_matches(|c: char|
        matches!(c, '.' | ',' | ';' | ':' | ')' | ']' | '>' | '!' | '?' | '"' | '\'')).len();
    (&s[..cut], &s[cut..])
}

fn is_email_like(word: &str) -> bool {
    let core = word.trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end_matches(|c: char| !c.is_alphanumeric());
    if let Some(at) = core.find('@') {
        let (user, domain) = core.split_at(at);
        let domain = &domain[1..];
        !user.is_empty() && domain.contains('.') && !domain.starts_with('.')
    } else {
        false
    }
}
