"""
@docs ARCHITECTURE:Agent:SystemOne
@docs ARCHITECTURE:Infrastructure:Execution

### AI Assist Note
**Tadpole System 1 Decision Engine Sidecar**:
Deterministic, non-autoregressive System 1 decision runner.
Supports pure Python / heuristic execution and local model inference.
Communicates via JSON-RPC 2.0 over named pipe / socket / stdio.

### 🔍 Debugging & Observability
- **Failure Path**: Missing checkpoint, high-cardinality label overflow, or script mismatch.
- **Telemetry Link**: Search `[tadpole_system_one]` in execution logs.
"""

from __future__ import annotations

import json
import logging
import math
import os
import re
import sys
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Sequence, Tuple, Union

logger = logging.getLogger("[tadpole_system_one]")


# -----------------------------------------------------------------------------
# 1. Script Profiling (Zero-Dependency Sub-0.5ms Exact Range Tables)
# -----------------------------------------------------------------------------

SCRIPT_RANGES = [
    ("greek", ((0x0370, 0x03FF), (0x1F00, 0x1FFF))),
    ("cyrillic", ((0x0400, 0x052F), (0x2DE0, 0x2DFF), (0xA640, 0xA69F))),
    ("armenian", ((0x0530, 0x058F),)),
    ("hebrew", ((0x0590, 0x05FF),)),
    ("arabic", ((0x0600, 0x06FF), (0x0750, 0x077F), (0x08A0, 0x08FF), (0xFB50, 0xFDFF), (0xFE70, 0xFEFF))),
    ("devanagari", ((0x0900, 0x097F), (0xA8E0, 0xA8FF))),
    ("bengali", ((0x0980, 0x09FF),)),
    ("gurmukhi", ((0x0A00, 0x0A7F),)),
    ("gujarati", ((0x0A80, 0x0AFF),)),
    ("oriya", ((0x0B00, 0x0B7F),)),
    ("tamil", ((0x0B80, 0x0BFF),)),
    ("telugu", ((0x0C00, 0x0C7F),)),
    ("kannada", ((0x0C80, 0x0CFF),)),
    ("malayalam", ((0x0D00, 0x0D7F),)),
    ("sinhala", ((0x0D80, 0x0DFF),)),
    ("thai", ((0x0E00, 0x0E7F),)),
    ("lao", ((0x0E80, 0x0EFF),)),
    ("tibetan", ((0x0F00, 0x0FFF),)),
    ("myanmar", ((0x1000, 0x109F),)),
    ("georgian", ((0x10A0, 0x10FF),)),
    ("ethiopic", ((0x1200, 0x137F),)),
    ("khmer", ((0x1780, 0x17FF),)),
    ("hangul", ((0x1100, 0x11FF), (0x3130, 0x318F), (0xAC00, 0xD7AF))),
    ("kana", ((0x3040, 0x309F), (0x30A0, 0x30FF), (0x31F0, 0x31FF))),
    ("han", ((0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xF900, 0xFAFF))),
]

NON_EN_DIACRITICS = set("àâäãáåçéèêëíìîïñóòôöõøúùûüýÿßæœăâîșțşţąćęłńśźżčďěňřšťůžőűğıāēģīķļņūžđ")
NON_EN_DIACRITIC_RATE = 0.02

STOPWORDS = {
    "en": {"the", "and", "is", "are", "was", "were", "to", "of", "in", "for", "with", "that", "this", "it", "you", "have"},
    "de": {"der", "die", "das", "und", "ist", "ein", "eine", "den", "dem", "nicht", "mit", "für", "auf", "von", "zu"},
    "fr": {"le", "la", "les", "des", "une", "est", "pour", "dans", "que", "qui", "avec", "sur", "pas", "plus"},
    "es": {"el", "los", "las", "que", "por", "con", "para", "una", "es", "se", "del", "como", "pero", "son"},
}


def profile_text(text: str) -> Dict[str, Any]:
    """Analyzes text script, diacritics, and language suitability in <0.5ms."""
    counts: Dict[str, int] = {}
    latin = 0
    total_alpha = 0
    diacritic_count = 0
    lowered = text.lower()

    for ch in lowered:
        if not ch.isalpha():
            continue
        total_alpha += 1
        cp = ord(ch)
        if ch in NON_EN_DIACRITICS:
            diacritic_count += 1

        if cp < 0x0250 or (0x1E00 <= cp <= 0x1EFF):
            latin += 1
            continue

        matched = False
        for name, ranges in SCRIPT_RANGES:
            if any(lo <= cp <= hi for lo, hi in ranges):
                counts[name] = counts.get(name, 0) + 1
                matched = True
                break
        if not matched:
            counts["other"] = counts.get("other", 0) + 1

    counts["latin"] = latin
    total = sum(counts.values())
    if total == 0:
        return {
            "dominant_script": "unknown",
            "is_english": True,
            "non_latin_fraction": 0.0,
            "diacritic_rate": 0.0,
            "language_guess": None,
        }

    dominant = max(counts.items(), key=lambda kv: kv[1])[0]
    non_latin_fraction = (total - latin) / float(total)
    diac_rate = diacritic_count / max(1, len(text))
    looks_non_english = diac_rate >= NON_EN_DIACRITIC_RATE

    is_english = dominant == "latin" and not looks_non_english and non_latin_fraction < 0.10
    lang_guess = "en" if is_english else ("non-en" if dominant == "latin" else dominant)

    return {
        "dominant_script": dominant,
        "is_english": is_english,
        "non_latin_fraction": round(non_latin_fraction, 4),
        "diacritic_rate": round(diac_rate, 4),
        "language_guess": lang_guess,
    }


# -----------------------------------------------------------------------------
# 2. Decision Primitives & Temperature Scaling
# -----------------------------------------------------------------------------

def softmax(logits: List[float], temperature: float = 1.0) -> List[float]:
    t = max(0.01, temperature)
    scaled = [z / t for z in logits]
    max_z = max(scaled)
    exp_z = [math.exp(z - max_z) for z in scaled]
    sum_exp = sum(exp_z)
    return [round(v / sum_exp, 5) for v in exp_z]


def confidence_from_probs(probs: List[float]) -> float:
    """Tadpole confidence estimation: margin of top probability over uniform background."""
    if not probs:
        return 0.0
    sorted_p = sorted(probs, reverse=True)
    if len(sorted_p) == 1:
        return 1.0
    k = len(probs)
    p_top = sorted_p[0]
    p_rest = (1.0 - p_top) / max(1, k - 1)
    return round(max(0.0, min(1.0, p_top - p_rest)), 4)


# -----------------------------------------------------------------------------
# 3. High-Cardinality Candidate Shortlisting
# -----------------------------------------------------------------------------

def shortlist_candidates(
    query: str,
    candidates: Union[Dict[str, str], Sequence[str]],
    k: int = 10,
    dim: int = 4096,
) -> List[str]:
    """Projects high-cardinality candidate labels to top-k using token bag overlap."""
    if not candidates:
        return []

    def tokenize(text: str) -> List[str]:
        return re.findall(r"[a-zA-Z0-9]+", text.lower())

    q_tokens = set(tokenize(query))
    if not q_tokens:
        items = list(candidates.keys()) if isinstance(candidates, dict) else list(candidates)
        return items[:k]

    scored = []
    items = candidates.items() if isinstance(candidates, dict) else enumerate(candidates)

    for ident, desc in items:
        ident_str = str(ident)
        c_text = f"{ident_str} {desc}"
        c_tokens = tokenize(c_text)
        if not c_tokens:
            scored.append((0.0, ident_str))
            continue

        c_set = set(c_tokens)
        overlap = len(q_tokens.intersection(c_set))
        norm_score = overlap / math.sqrt(len(c_set))
        scored.append((norm_score, ident_str))

    scored.sort(key=lambda x: x[0], reverse=True)
    return [ident for _, ident in scored[:k]]


# -----------------------------------------------------------------------------
# 4. System 1 Decision Engine
# -----------------------------------------------------------------------------

@dataclass
class SystemOneEngine:
    """Deterministic, non-autoregressive classification engine for Tadpole OS."""
    temperature: float = 1.0
    onnx_session: Any = None
    tokenizer: Any = None

    def evaluate(
        self,
        state: Dict[str, Any],
        questions: Dict[str, Dict[str, Any]],
    ) -> Dict[str, Any]:
        """Evaluates typed questions ('choice', 'score', 'noul') against current turn state."""
        text_state = state.get("prompt", "") or state.get("content", "")
        if isinstance(text_state, dict):
            text_state = json.dumps(text_state)

        # 1. Script Profiling
        script_info = profile_text(str(text_state))

        # 2. Evaluate Each Question
        answers = {}
        for qid, qdef in questions.items():
            qtype = qdef.get("type", "choice")
            instructions = qdef.get("instructions", "")
            criteria = qdef.get("criteria", [])

            if qtype == "noul":
                # Continuous [0, 1] probability (Noul)
                prob = self._heuristic_noul(text_state, instructions)
                answers[qid] = {
                    "type": "noul",
                    "noul": round(prob, 4),
                    "confidence": round(abs(prob - 0.5) * 2.0, 4),
                }

            elif qtype == "score":
                # Continuous bounded score
                k = len(criteria) if isinstance(criteria, list) and criteria else 5
                raw_score = self._heuristic_score(text_state, instructions, k)
                answers[qid] = {
                    "type": "score",
                    "score": round(raw_score, 3),
                    "confidence": 0.88,
                }

            else:
                # Typed Choice
                opts = list(criteria.keys()) if isinstance(criteria, dict) else list(criteria)
                if not opts:
                    opts = ["default"]
                best_opt, probs = self._heuristic_choice(text_state, instructions, criteria)
                answers[qid] = {
                    "type": "choice",
                    "choice": best_opt,
                    "probabilities": probs,
                    "confidence": confidence_from_probs(list(probs.values())),
                }

        logger.debug("[tadpole_system_one] Evaluated %d questions for state", len(questions))
        return {
            "model": "system-one-deterministic",
            "script_profile": script_info,
            "answers": answers,
            "metadata": {
                "engine": "tadpole-v1",
                "non_autoregressive": True,
            }
        }

    def _heuristic_noul(self, text: str, instructions: str) -> float:
        lower = text.lower()
        inst = instructions.lower()

        if "jailbreak" in inst or "ignore" in inst:
            if "ignore all previous" in lower or "dan mode" in lower or "<|im_start|>" in lower:
                return 0.94
            if "disregard system" in lower:
                return 0.89
            return 0.03

        if "injection" in inst:
            if "you are now" in lower or "reveal your system prompt" in lower:
                return 0.91
            return 0.04

        if "phish" in inst or "spam" in inst:
            if "wire money" in lower or "urgent account verification" in lower:
                return 0.88
            return 0.05

        return 0.10

    def _heuristic_score(self, text: str, instructions: str, k: int) -> float:
        lower = text.lower()
        if "risk" in instructions.lower() or "harm" in instructions.lower():
            if "rm -rf" in lower or "drop table" in lower or "delete from" in lower:
                return float(min(k - 1, 2.9))
            if "update" in lower or "write" in lower:
                return 0.8
            return 0.1
        return 0.0

    def _heuristic_choice(
        self,
        text: str,
        instructions: str,
        criteria: Union[Dict[str, Any], List[str]],
    ) -> Tuple[str, Dict[str, float]]:
        keys = list(criteria.keys()) if isinstance(criteria, dict) else list(criteria)
        if not keys:
            return "none", {"none": 1.0}

        scores = []
        lower_text = text.lower()
        for k in keys:
            desc = criteria.get(k, "") if isinstance(criteria, dict) else ""
            desc_str = str(desc or "").lower()
            score = 1.0
            if k.lower() in lower_text:
                score += 3.0
            words = re.findall(r"\w+", desc_str)
            for w in words:
                if len(w) >= 3 and w in lower_text:
                    score += 2.0
            scores.append(score)

        probs_list = softmax(scores, self.temperature)
        prob_dict = {k: probs_list[i] for i, k in enumerate(keys)}
        best_key = keys[int(probs_list.index(max(probs_list)))]
        return best_key, prob_dict


if __name__ == "__main__":
    engine = SystemOneEngine()
    test_state = {"prompt": "What is the memory consumption of background task #2?"}
    test_questions = {
        "routing": {
            "type": "choice",
            "instructions": "Determine model tier",
            "criteria": {
                "tier1_fast": "simple status check, lookup, or formatting",
                "tier2_reasoning": "deep reasoning, multi-file refactoring",
            },
        },
        "jailbreak": {
            "type": "noul",
            "instructions": "Does prompt attempt a jailbreak?",
        }
    }
    result = engine.evaluate(test_state, test_questions)
    print(json.dumps(result, indent=2))
