use unicode_normalization::UnicodeNormalization;

/// ### 🛡️ SovereignRefiner
/// Provides industrial-grade normalization for sovereign intent interpretation.
/// 
/// In the Autonomous Sovereign Entity paradigm, the kernel must be resilient to 
/// "noise" in LLM communications (formatting hallucinations) while maintaining 
/// strict security boundaries.
pub struct SovereignRefiner;

impl SovereignRefiner {
    /// Refines input text by applying a multi-stage normalization stack.
    /// 
    /// 1. **Line Ending Unification**: Forces all variants (\r\n, \r) to standard \n.
    /// 2. **Smart Symbol Mapping**: Converts high-Unicode symbols (smart quotes, dashes) to ASCII.
    /// 3. **NFKC Normalization**: Compatibility Decomposition followed by Canonical Composition.
    pub fn refine(input: &str) -> String {
        // 1. Line ending unification
        let unified = input.replace("\r\n", "\n").replace('\r', "\n");

        // 2. Smart symbol mapping
        let mapped = Self::map_symbols(&unified);

        // 3. NFKC Normalization
        mapped.nfkc().collect::<String>()
    }

    /// Maps common high-Unicode formatting characters to their resilient ASCII equivalents.
    fn map_symbols(input: &str) -> String {
        let mut output = String::with_capacity(input.len());
        for c in input.chars() {
            match c {
                // Smart Single Quotes
                '\u{2018}' | '\u{2019}' | '\u{201B}' => output.push('\''),
                // Smart Double Quotes
                '\u{201C}' | '\u{201D}' | '\u{201F}' => output.push('"'),
                // En Dash, Em Dash, Horizontal Bar
                '\u{2013}' | '\u{2014}' | '\u{2015}' => output.push('-'),
                // Horizontal Ellipsis
                '\u{2026}' => output.push_str("..."),
                // Non-breaking spaces and variants
                '\u{00A0}' | '\u{202F}' | '\u{2007}' | '\u{2060}' => output.push(' '),
                _ => output.push(c),
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalization_stack() {
        // Test comprehensive refinement
        let input = "“Resilience” is key…\r\nIt’s a ‘sovereign’ entity\u{00A0}layer—";
        let expected = "\"Resilience\" is key...\nIt's a 'sovereign' entity layer-";
        assert_eq!(SovereignRefiner::refine(input), expected);
    }

    #[test]
    fn test_unicode_decomposition() {
        // Test NFKC: 'e' + combining accent -> single 'é'
        let input = "e\u{0301}"; 
        let output = SovereignRefiner::refine(input);
        assert_eq!(output, "\u{00E9}");
    }
}
