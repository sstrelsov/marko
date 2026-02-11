//! LaTeX-to-Unicode conversion for inline and display math.
//!
//! Converts common LaTeX commands (Greek letters, operators, arrows, etc.)
//! to their Unicode equivalents, and handles superscript/subscript notation.

/// Convert LaTeX math to Unicode approximation.
pub fn latex_to_unicode(input: &str) -> String {
    let mut s = input.to_string();

    // Greek lowercase
    let replacements = [
        ("\\alpha", "α"), ("\\beta", "β"), ("\\gamma", "γ"), ("\\delta", "δ"),
        ("\\epsilon", "ε"), ("\\varepsilon", "ε"), ("\\zeta", "ζ"), ("\\eta", "η"),
        ("\\theta", "θ"), ("\\vartheta", "ϑ"), ("\\iota", "ι"), ("\\kappa", "κ"),
        ("\\lambda", "λ"), ("\\mu", "μ"), ("\\nu", "ν"), ("\\xi", "ξ"),
        ("\\pi", "π"), ("\\rho", "ρ"), ("\\sigma", "σ"), ("\\tau", "τ"),
        ("\\upsilon", "υ"), ("\\phi", "φ"), ("\\varphi", "ϕ"), ("\\chi", "χ"),
        ("\\psi", "ψ"), ("\\omega", "ω"),
        // Greek uppercase
        ("\\Gamma", "Γ"), ("\\Delta", "Δ"), ("\\Theta", "Θ"), ("\\Lambda", "Λ"),
        ("\\Xi", "Ξ"), ("\\Pi", "Π"), ("\\Sigma", "Σ"), ("\\Phi", "Φ"),
        ("\\Psi", "Ψ"), ("\\Omega", "Ω"),
        // Operators
        ("\\int", "∫"), ("\\iint", "∬"), ("\\iiint", "∭"),
        ("\\oint", "∮"), ("\\sum", "∑"), ("\\prod", "∏"),
        ("\\sqrt", "√"), ("\\partial", "∂"), ("\\nabla", "∇"), ("\\infty", "∞"),
        // Relations
        ("\\leq", "≤"), ("\\geq", "≥"), ("\\neq", "≠"), ("\\approx", "≈"),
        ("\\equiv", "≡"), ("\\sim", "∼"), ("\\propto", "∝"),
        ("\\pm", "±"), ("\\mp", "∓"), ("\\times", "×"), ("\\div", "÷"),
        ("\\cdot", "·"), ("\\circ", "∘"), ("\\star", "⋆"),
        // Arrows
        ("\\rightarrow", "→"), ("\\leftarrow", "←"), ("\\leftrightarrow", "↔"),
        ("\\Rightarrow", "⇒"), ("\\Leftarrow", "⇐"), ("\\Leftrightarrow", "⇔"),
        ("\\to", "→"), ("\\mapsto", "↦"),
        ("\\uparrow", "↑"), ("\\downarrow", "↓"),
        // Sets & logic
        ("\\in", "∈"), ("\\notin", "∉"), ("\\subset", "⊂"), ("\\supset", "⊃"),
        ("\\subseteq", "⊆"), ("\\supseteq", "⊇"), ("\\cup", "∪"), ("\\cap", "∩"),
        ("\\emptyset", "∅"), ("\\varnothing", "∅"),
        ("\\forall", "∀"), ("\\exists", "∃"), ("\\nexists", "∄"),
        ("\\neg", "¬"), ("\\wedge", "∧"), ("\\vee", "∨"),
        // Dots
        ("\\ldots", "…"), ("\\cdots", "⋯"), ("\\dots", "…"), ("\\vdots", "⋮"),
        // Misc
        ("\\hbar", "ℏ"), ("\\ell", "ℓ"), ("\\Re", "ℜ"), ("\\Im", "ℑ"),
        ("\\aleph", "ℵ"), ("\\wp", "℘"), ("\\degree", "°"),
        // Spacing/formatting (remove)
        ("\\quad", " "), ("\\qquad", "  "), ("\\,", ""), ("\\;", " "),
        ("\\!", ""), ("\\left", ""), ("\\right", ""), ("\\big", ""),
        ("\\Big", ""), ("\\bigg", ""), ("\\Bigg", ""),
    ];
    for (cmd, repl) in &replacements {
        s = s.replace(cmd, repl);
    }

    // Handle \frac{a}{b} → a⁄b
    while let Some(start) = s.find("\\frac{") {
        let after_frac = start + 6;
        if let Some(close1) = find_matching_brace(&s, after_frac) {
            let numer = s[after_frac..close1].to_string();
            let after_close1 = close1 + 1;
            if s[after_close1..].starts_with('{') {
                if let Some(close2) = find_matching_brace(&s, after_close1 + 1) {
                    let denom = s[after_close1 + 1..close2].to_string();
                    let replacement = if numer.len() == 1 && denom.len() == 1 {
                        format!("{}⁄{}", numer, denom)
                    } else {
                        format!("({}⁄{})", numer, denom)
                    };
                    s = format!("{}{}{}", &s[..start], replacement, &s[close2 + 1..]);
                    continue;
                }
            }
        }
        break;
    }

    // Handle ^{expr} → superscript and _{expr} → subscript
    process_super_sub(&s)
}

/// Process ^{} and _{} groups in a string, recursively handling nested groups.
/// Only converts to Unicode super/subscript if ALL chars in the group have equivalents.
fn process_super_sub(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '^' || chars[i] == '_' {
            let is_super = chars[i] == '^';
            let can_convert = if is_super { can_superscript } else { can_subscript };
            let convert = if is_super { to_superscript } else { to_subscript };

            if i + 1 < chars.len() && chars[i + 1] == '{' {
                // ^{expr} or _{expr}
                let brace_start = i + 2;
                if let Some(brace_end) = find_matching_brace_chars(&chars, brace_start) {
                    let raw: String = chars[brace_start..brace_end].iter().collect();
                    // Recursively process inner content first
                    let inner = process_super_sub(&raw);
                    if can_convert(&inner) {
                        result.push_str(&convert(&inner));
                    } else {
                        result.push(chars[i]);
                        result.push('(');
                        result.push_str(&inner);
                        result.push(')');
                    }
                    i = brace_end + 1;
                    continue;
                }
            } else if i + 1 < chars.len() && chars[i + 1] != ' ' {
                // ^x or _x (single char)
                let ch = chars[i + 1].to_string();
                if can_convert(&ch) {
                    result.push_str(&convert(&ch));
                } else {
                    result.push(chars[i]);
                    result.push(chars[i + 1]);
                }
                i += 2;
                continue;
            }
        }
        if chars[i] == '{' || chars[i] == '}' {
            // Strip bare grouping braces
            i += 1;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn can_superscript(s: &str) -> bool {
    s.chars().all(|c| matches!(c, '0'..='9' | '+' | '-' | '=' | '(' | ')' | 'n' | 'i'))
}

fn can_subscript(s: &str) -> bool {
    s.chars().all(|c| matches!(c,
        '0'..='9' | '+' | '-' | '=' | '(' | ')' |
        'a' | 'e' | 'o' | 'x' | 'h' | 'k' | 'l' | 'm' | 'n' | 'p' | 's' | 't'
    ))
}

fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_matching_brace_chars(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 1;
    for i in start..chars.len() {
        match chars[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn to_superscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴',
            '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
            '+' => '⁺', '-' => '⁻', '=' => '⁼', '(' => '⁽', ')' => '⁾',
            'n' => 'ⁿ', 'i' => 'ⁱ',
            _ => c,
        })
        .collect()
}

pub fn to_subscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄',
            '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
            '+' => '₊', '-' => '₋', '=' => '₌', '(' => '₍', ')' => '₎',
            'a' => 'ₐ', 'e' => 'ₑ', 'o' => 'ₒ', 'x' => 'ₓ',
            'h' => 'ₕ', 'k' => 'ₖ', 'l' => 'ₗ', 'm' => 'ₘ',
            'n' => 'ₙ', 'p' => 'ₚ', 's' => 'ₛ', 't' => 'ₜ',
            _ => c,
        })
        .collect()
}
