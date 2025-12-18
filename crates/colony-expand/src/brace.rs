pub fn expand_braces(content: &str) -> Vec<String> {
    if content.contains("..") {
        expand_range(content)
    } else if content.contains(',') {
        expand_comma_list(content)
    } else {
        vec![format!("{{{}}}", content)]
    }
}

fn expand_range(content: &str) -> Vec<String> {
    let parts: Vec<&str> = content.splitn(2, "..").collect();
    if parts.len() != 2 {
        return vec![format!("{{{}}}", content)];
    }
    
    let start = parts[0].trim();
    let end = parts[1].trim();
    
    if let (Ok(start_num), Ok(end_num)) = (start.parse::<i32>(), end.parse::<i32>()) {
        return expand_numeric_range(start_num, end_num);
    }
    
    if start.len() == 1 && end.len() == 1 {
        let start_char = start.chars().next().unwrap();
        let end_char = end.chars().next().unwrap();
        
        if start_char.is_ascii_alphabetic() && end_char.is_ascii_alphabetic() {
            return expand_alpha_range(start_char, end_char);
        }
    }
    
    vec![format!("{{{}}}", content)]
}

fn expand_numeric_range(start: i32, end: i32) -> Vec<String> {
    let range: Vec<i32> = if start <= end {
        (start..=end).collect()
    } else {
        (end..=start).rev().collect()
    };
    
    range.into_iter().map(|n| n.to_string()).collect()
}

fn expand_alpha_range(start: char, end: char) -> Vec<String> {
    let start_code = start as u8;
    let end_code = end as u8;
    
    let range: Vec<u8> = if start_code <= end_code {
        (start_code..=end_code).collect()
    } else {
        (end_code..=start_code).rev().collect()
    };
    
    range.into_iter().map(|c| (c as char).to_string()).collect()
}

fn expand_comma_list(content: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut current = String::new();
    let mut brace_depth = 0;
    
    for c in content.chars() {
        match c {
            '{' => {
                brace_depth += 1;
                current.push(c);
            }
            '}' => {
                brace_depth -= 1;
                current.push(c);
            }
            ',' if brace_depth == 0 => {
                results.push(current.trim().to_string());
                current = String::new();
            }
            _ => {
                current.push(c);
            }
        }
    }
    
    if !current.is_empty() {
        results.push(current.trim().to_string());
    }
    
    results
}

pub fn expand_word_with_braces(prefix: &str, braces: &str, suffix: &str) -> Vec<String> {
    let inner = braces.trim_start_matches('{').trim_end_matches('}');
    let expansions = expand_braces(inner);
    
    expansions.into_iter()
        .map(|exp| format!("{}{}{}", prefix, exp, suffix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comma_expansion() {
        assert_eq!(expand_braces("a,b,c"), vec!["a", "b", "c"]);
        assert_eq!(expand_braces("one,two,three"), vec!["one", "two", "three"]);
    }

    #[test]
    fn test_numeric_range() {
        assert_eq!(expand_braces("1..5"), vec!["1", "2", "3", "4", "5"]);
        assert_eq!(expand_braces("5..1"), vec!["5", "4", "3", "2", "1"]);
    }

    #[test]
    fn test_alpha_range() {
        assert_eq!(expand_braces("a..e"), vec!["a", "b", "c", "d", "e"]);
        assert_eq!(expand_braces("e..a"), vec!["e", "d", "c", "b", "a"]);
    }

    #[test]
    fn test_with_prefix_suffix() {
        assert_eq!(
            expand_word_with_braces("file", "{1,2}", ".txt"),
            vec!["file1.txt", "file2.txt"]
        );
        assert_eq!(
            expand_word_with_braces("pre", "{a,b}", "suf"),
            vec!["preasuf", "prebsuf"]
        );
    }
}
