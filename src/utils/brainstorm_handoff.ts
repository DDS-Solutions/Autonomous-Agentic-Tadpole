/**
 * @docs ARCHITECTURE:Logic
 *
 * ### AI Assist Note
 * **Brainstorm Handoff Formatter**: Formats recent conversational turns from Chat/Plan mode
 * into a compact bounded context block passed via `recent_findings` across turns and on
 * Brainstorm -> Swarm Mission handoff.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Oversized transcripts exceeding prompt budget if bounds are bypassed.
 * - **Telemetry Link**: Search `[brainstorm_handoff]` in UI traces.
 */

export interface Brainstorm_Entry {
    speaker: string;
    text: string;
    is_system?: boolean;
}

const DEFAULT_MAX_TURNS = 12;
const DEFAULT_MAX_CHARS = 2400;

/**
 * Formats recent conversational turns from Chat/Plan mode into a compact
 * bounded context block passed via `recent_findings` across turns and on
 * Brainstorm -> Swarm Mission handoff.
 */
export function build_brainstorm_context(
    entries: Brainstorm_Entry[],
    max_turns: number = DEFAULT_MAX_TURNS,
    max_chars: number = DEFAULT_MAX_CHARS
): string | undefined {
    const conversational = entries
        .filter((e) => !e.is_system && Boolean(e.text?.trim()))
        .slice(-max_turns);

    if (conversational.length === 0) return undefined;

    const lines = conversational.map((e) => {
        const cleaned = e.text.replace(/\s+/g, ' ').trim();
        return `${e.speaker}: ${cleaned}`;
    });

    const header = '### Prior Brainstorm Context (Chat/Plan Session)\n';
    let body = lines.join('\n');
    const available = Math.max(120, max_chars - header.length);
    if (body.length > available) {
        body = `...${body.slice(body.length - (available - 3))}`;
    }

    return `${header}${body}`;
}

// Metadata: [brainstorm_handoff]
