"""
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**dedupe_headers**: Scans and cleans duplicate AI header blocks and metadata footer tags in the codebase.
This script ensures compliance with the Sovereign Awakening standard by keeping exactly one header and footer.

### 🔍 Debugging & Observability
- **Failure Path**: Permission error, file write failure, or regex mismatch.
- **Telemetry Link**: Search `[dedupe_headers]` in system logs.
"""

import os
import re
import argparse
import sys
import io
from pathlib import Path

# Ensure stdout handles UTF-8 on Windows
if sys.platform == "win32":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')

ROOT = Path(__file__).resolve().parent.parent
TARGET_DIRS = ['.']

# Header regexes by file extension
HEADER_PATTERNS = {
    '.rs': r'(?://! @docs ARCHITECTURE:[^\n]+\n(?://!.*\n)*\n*)',
    '.ts': r'(?:/\*\*\n \* @docs ARCHITECTURE:[^\n]+\n(?: \*(?:(?!/\*|\*/).)*\n)* \*/\n*)',
    '.tsx': r'(?:/\*\*\n \* @docs ARCHITECTURE:[^\n]+\n(?: \*(?:(?!/\*|\*/).)*\n)* \*/\n*)',
    '.js': r'(?:/\*\*\n \* @docs ARCHITECTURE:[^\n]+\n(?: \*(?:(?!/\*|\*/).)*\n)* \*/\n*)',
    '.css': r'(?:/\*\*\n \* @docs ARCHITECTURE:[^\n]+\n(?: \*(?:(?!/\*|\*/).)*\n)* \*/\n*)',
    '.py': r'(?:"""\n@docs ARCHITECTURE:[^\n]+\n(?:(?:(?!""").)*\n)*?"""\n*)',
    '.md': r'(?:> \[!IMPORTANT\]\n> \*\*AI Assist Note[^\n]+\n(?:>.*\n)*\n*)',
    '.html': r'(?:<!--\n  @docs ARCHITECTURE:[^\n]+\n(?:(?:(?!-->).)*\n)*?-->\n*)',
}

# Footer metadata tag regexes by extension
FOOTER_PATTERNS = {
    '.rs': r'// Metadata: \[[a-zA-Z0-9_-]+\]',
    '.ts': r'// Metadata: \[[a-zA-Z0-9_-]+\]',
    '.tsx': r'// Metadata: \[[a-zA-Z0-9_-]+\]',
    '.js': r'// Metadata: \[[a-zA-Z0-9_-]+\]',
    '.css': r'// Metadata: \[[a-zA-Z0-9_-]+\]',
    '.py': r'# Metadata: \[[a-zA-Z0-9_-]+\]',
    '.sh': r'# Metadata: \[[a-zA-Z0-9_-]+\]',
    '.md': r'\[//\]: # \(Metadata: \[[a-zA-Z0-9_-]+\]\)',
    '.html': r'<!-- Metadata: \[[a-zA-Z0-9_-]+\] -->',
}

GENERIC_PHRASES = [
    "Core technical resource for the Tadpole OS infrastructure.",
    "UI errors or callback stack traces.",
    "Script crash or unexpected exception.",
    "Information drift or legacy terminology.",
    "Handles reactive state and high-fidelity user interactions.",
    "This module implements high-fidelity logic for the Sovereign Reality layer."
]

def select_best_header(matches):
    # Extract the matched text blocks
    headers = [m.group(0) for m in matches]
    scored_headers = []
    for h in headers:
        # Lower score if it contains generic phrases
        is_generic = any(phrase in h for phrase in GENERIC_PHRASES)
        score = 0 if is_generic else 15
        
        # Add points for length (more detailed information)
        score += len(h) // 20
        
        # Add points for specific @docs tag that matches the module rather than just "Core"
        docs_match = re.search(r'@docs\s+ARCHITECTURE:([a-zA-Z0-9_:]+)', h)
        if docs_match and docs_match.group(1).lower() != "core":
            score += 25
            
        scored_headers.append((score, h))
        
    # Sort by score descending
    scored_headers.sort(key=lambda x: x[0], reverse=True)
    return scored_headers[0][1]

def merge_header_blocks(chosen, others, ext):
    # Check if chosen has both sections using the same regexes as verify_ai_context.py
    has_note = re.search(r'###\s+AI\s+Assist\s+Note', chosen) is not None
    has_debugging = re.search(r'###\s+.*?\s+Debugging\s+&\s+Observability', chosen) is not None
    
    if has_note and has_debugging:
        return chosen
        
    # If chosen is missing debugging, try to find it in others
    if not has_debugging:
        debugging_block = None
        for other in others:
            if other == chosen:
                continue
            if ext == '.rs':
                match = re.search(r'(//!\s*###\s*(?:🔍\s*)?Debugging\s*&\s*Observability\n(?://!.*\n)*)', other)
                if match:
                    debugging_block = match.group(1)
                    break
            elif ext in ('.ts', '.tsx', '.js', '.css'):
                match = re.search(r'( \*\s*###\s*(?:🔍\s*)?Debugging\s*&\s*Observability\n(?: \*(?!\*/).*\n)*)', other)
                if match:
                    debugging_block = match.group(1)
                    break
            elif ext == '.py':
                match = re.search(r'(###\s*(?:🔍\s*)?Debugging\s*&\s*Observability\n(?:(?!""").*\n)*)', other)
                if match:
                    debugging_block = match.group(1)
                    break
            elif ext == '.md':
                match = re.search(r'(>\s*###\s*(?:🔍\s*)?Debugging\s*&\s*Observability\n(?:>.*\n)*)', other)
                if match:
                    debugging_block = match.group(1)
                    break
                    
        if debugging_block:
            # Append debugging block to chosen
            if ext == '.rs':
                chosen = chosen.rstrip() + "\n//!\n" + debugging_block
            elif ext in ('.ts', '.tsx', '.js', '.css'):
                chosen = chosen.replace(" */", f" *\n{debugging_block} */")
            elif ext == '.py':
                chosen = chosen.replace('"""', f'\n{debugging_block}"""')
            elif ext == '.md':
                chosen = chosen.rstrip() + "\n>\n" + debugging_block

    # If chosen is missing note, try to find it in others
    if not has_note:
        note_block = None
        for other in others:
            if other == chosen:
                continue
            if ext == '.rs':
                match = re.search(r'(//!\s*###\s*AI\s*Assist\s*Note\n(?://!.*\n)*)', other)
                if match:
                    note_block = match.group(1)
                    break
            elif ext in ('.ts', '.tsx', '.js', '.css'):
                match = re.search(r'( \*\s*###\s*AI\s*Assist\s*Note\n(?: \*(?!\*/).*\n)*)', other)
                if match:
                    note_block = match.group(1)
                    break
            elif ext == '.py':
                match = re.search(r'(###\s*AI\s*Assist\s*Note\n(?:(?!""").*\n)*)', other)
                if match:
                    note_block = match.group(1)
                    break
            elif ext == '.md':
                match = re.search(r'(>\s*###\s*AI\s*Assist\s*Note\n(?:>.*\n)*)', other)
                if match:
                    note_block = match.group(1)
                    break
                    
        if note_block:
            # Prepend or insert note block in chosen
            if ext == '.rs':
                # Insert right after the @docs tag line if present
                lines = chosen.splitlines()
                idx = 0
                for i, l in enumerate(lines):
                    if "@docs" in l:
                        idx = i + 1
                        break
                lines.insert(idx, "//!\n" + note_block.rstrip())
                chosen = "\n".join(lines) + "\n"
            elif ext in ('.ts', '.tsx', '.js', '.css'):
                lines = chosen.splitlines()
                idx = 1
                for i, l in enumerate(lines):
                    if "@docs" in l:
                        idx = i + 1
                        break
                lines.insert(idx, " *\n" + note_block.rstrip())
                chosen = "\n".join(lines) + "\n"
            elif ext == '.py':
                lines = chosen.splitlines()
                idx = 1
                for i, l in enumerate(lines):
                    if "@docs" in l:
                        idx = i + 1
                        break
                lines.insert(idx, "\n" + note_block.rstrip())
                chosen = "\n".join(lines) + "\n"
            elif ext == '.md':
                lines = chosen.splitlines()
                idx = 1
                for i, l in enumerate(lines):
                    if "@docs" in l:
                        idx = i + 1
                        break
                lines.insert(idx, ">\n" + note_block.rstrip())
                chosen = "\n".join(lines) + "\n"
                
    return chosen

def clean_file(file_path, dry_run=False):
    ext = os.path.splitext(file_path)[1]
    if ext not in HEADER_PATTERNS and ext not in FOOTER_PATTERNS:
        return False, None
        
    try:
        with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()
    except Exception as e:
        return False, f"Read Error: {e}"

    original_content = content
    modified = False
    
    # 1. Deduplicate Header (with intelligent selection & merging)
    if ext in HEADER_PATTERNS:
        pattern_str = HEADER_PATTERNS[ext]
        matches = list(re.finditer(pattern_str, content))
        if len(matches) > 1:
            # Select the best header based on specificity
            best_header = select_best_header(matches)
            
            # Merge missing sections if needed
            headers_list = [m.group(0) for m in matches]
            merged_header = merge_header_blocks(best_header, headers_list, ext)
            
            # Remove all header matches from the content
            for m in reversed(matches):
                start, end = m.span()
                content = content[:start] + content[end:]
            
            # Prepend the selected best header to the top
            content = merged_header + content.lstrip()
            modified = True

    # 2. Deduplicate Footer Metadata Tags (grouped by value to avoid breaking telemetry tags)
    if ext in FOOTER_PATTERNS:
        pattern_str = FOOTER_PATTERNS[ext]
        matches = list(re.finditer(pattern_str, content))
        if len(matches) > 1:
            # Group matches by the exact tag string (e.g. '// Metadata: [merge]')
            tag_to_matches = {}
            for m in matches:
                tag_text = m.group(0)
                tag_to_matches.setdefault(tag_text, []).append(m)
                
            spans_to_delete = []
            for tag_text, tag_matches in tag_to_matches.items():
                if len(tag_matches) > 1:
                    # Mark all but the last occurrence of this specific tag value for deletion
                    for m in tag_matches[:-1]:
                        spans_to_delete.append(m.span())
                        
            if spans_to_delete:
                # Delete spans in reverse order to preserve offsets
                spans_to_delete.sort(key=lambda x: x[0], reverse=True)
                for start, end in spans_to_delete:
                    content = content[:start] + content[end:]
                modified = True
            
    if modified and not dry_run:
        try:
            with open(file_path, 'w', encoding='utf-8') as f:
                f.write(content)
        except Exception as e:
            return False, f"Write Error: {e}"
            
    return modified, None

def main():
    parser = argparse.ArgumentParser(description="Clean duplicate AI headers/footers in Tadpole OS.")
    parser.add_argument("--dry-run", action="store_true", help="Audit mode (no file changes)")
    args = parser.parse_args()
    
    print(f"--- Tadpole OS: Header & Footer Deduplication Orchestrator {'(Dry Run)' if args.dry_run else ''} ---")
    
    cleaned_count = 0
    errors_count = 0
    
    for target in TARGET_DIRS:
        target_path = ROOT / target
        if not target_path.exists():
            continue
            
        for r, dirs, files in os.walk(target_path):
            # Prune directories in place to avoid walking into huge dependency or build directories
            dirs[:] = [d for d in dirs if d not in ['.git', 'node_modules', 'target', 'dist', '.tmp', 'reports', 'coverage', 'scratch', 'build', '.venv', 'venv']]
            for file in files:
                ext = os.path.splitext(file)[1]
                if ext not in HEADER_PATTERNS:
                    continue
                file_path = os.path.join(r, file)
                modified, err = clean_file(file_path, dry_run=args.dry_run)
                if err:
                    print(f"❌ Error in {file_path}: {err}")
                    errors_count += 1
                elif modified:
                    print(f"🧹 Cleaned duplicate markers in: {os.path.relpath(file_path, ROOT)}")
                    cleaned_count += 1
                    
    print(f"\nScan Complete. Cleaned: {cleaned_count} files. Errors: {errors_count}")

if __name__ == "__main__":
    main()

# Metadata: [dedupe_headers]
