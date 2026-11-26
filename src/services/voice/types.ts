/**
 * @docs ARCHITECTURE:AudioIntelligence
 *
 * ### AI Assist Note
 * **Voice Types**: Shared TypeScript interfaces for browser SpeechRecognition abstraction,
 * voice session lifecycle status, and supported TTS synthesis engines.
 *
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Type mismatch against browser SpeechRecognition events.
 * - **Trace Scope**: `src/services/voice/types` (pure type contracts; no runtime telemetry emitted).
 */

export interface Speech_Alternative {
    readonly transcript: string;
}

export interface Speech_Result {
    readonly length: number;
    [index: number]: Speech_Alternative;
}

export interface Speech_Result_List {
    readonly length: number;
    [index: number]: Speech_Result;
}

export interface Speech_Recognition_Interface {
    continuous: boolean;
    interimResults: boolean;
    lang: string;
    onresult: (event: { results: Speech_Result_List }) => void;
    onerror: (event: { error: string }) => void;
    onend: () => void;
    start: () => void;
    stop: () => void;
    abort?: () => void;
}

export type Voice_Status = 'idle' | 'initializing' | 'active' | 'stalled';

export type Voice_Tts_Engine = 'browser' | 'openai' | 'groq' | 'piper' | 'gemini-live';

// Metadata: [types]
