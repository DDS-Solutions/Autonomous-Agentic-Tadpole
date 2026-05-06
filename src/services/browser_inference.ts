/**
 * @docs ARCHITECTURE:UI-Services
 * 
 * ### AI Assist Note
 * **@docs ARCHITECTURE:Agent**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[browser_inference]` in observability traces.
 */

/**
 * @docs ARCHITECTURE:Agent
 * 
 * ### AI Assist Note
 * **Browser Inference Engine**: High-performance local AI specialist. 
 * Offloads reasoning tasks to the user's browser via WebGPU/Transformers.js.
 * Implements **Zero-Latency Monitoring**: allows the agent to analyze the 
 * active UI state without network roundtrips.
 * 
 * ### 🛡️ Security Mandate
 * - **Local-Only**: Models are downloaded once and cached. No inference 
 *   data is sent back to external APIs.
 * - **Sandboxed Context**: The model only sees sanitized DOM summaries.
 */

import { pipeline, env } from '@huggingface/transformers';

// Configure transformers.js to use local cache and wasm/webgpu
env.allowLocalModels = false;
env.useBrowserCache = true;

export type InferenceStatus = 'idle' | 'loading' | 'thinking' | 'error';

class BrowserInferenceService {
    private pipe: any = null;
    private embedding_pipe: any = null;
    private status: InferenceStatus = 'idle';
    private model_id: string = 'onnx-community/Gemma-2b-it-v2';
    private embed_model_id: string = 'onnx-community/all-MiniLM-L6-v2';

    /**
     * Initializes the local AI model.
     * Starts background download if not present in cache.
     */
    async init_specialist() {
        if (this.pipe && this.embedding_pipe) return;
        this.status = 'loading';
        console.log('🧠 [BrowserSpecialist] Initializing local AI specialist...');

        try {
            // Load both text generation and feature extraction (embeddings)
            const [gen, embed] = await Promise.all([
                pipeline('text-generation', this.model_id, { device: 'webgpu' }),
                pipeline('feature-extraction', this.embed_model_id, { device: 'webgpu' })
            ]);
            
            this.pipe = gen;
            this.embedding_pipe = embed;
            this.status = 'idle';
            console.log('🧠 [BrowserSpecialist] Local specialist & Embedding engine READY.');
        } catch (err) {
            console.error('🧠 [BrowserSpecialist] Initialization failed:', err);
            this.status = 'error';
            throw err;
        }
    }

    /**
     * Generates a vector embedding for the provided text.
     */
    async get_embedding(text: string): Promise<number[]> {
        if (!this.embedding_pipe) {
            await this.init_specialist();
        }

        try {
            const output = await this.embedding_pipe(text, {
                pooling: 'mean',
                normalize: true,
            });
            return Array.from(output.data);
        } catch (err) {
            console.error('🧠 [BrowserSpecialist] Embedding failed:', err);
            throw err;
        }
    }

    /**
     * Executes local reasoning based on a prompt and context.
     */
    async analyze_ui(prompt: string, dom_summary: string): Promise<string> {
        if (!this.pipe) {
            await this.init_specialist();
        }

        this.status = 'thinking';
        
        // Dynamic system prompt based on whether it's a direct user query or a sentinel scan
        const system_prompt = prompt.includes('SENTINEL_SCAN') 
            ? "You are a Sentinel Monitor. Detect UI anomalies, errors, or high entropy. If you find a critical issue, include 'ESCALATE_TO_ARCHITECT' in your response."
            : "You are a Browser Specialist Agent. Analyze the following UI state and answer the user query concisely.";

        const input = `SYSTEM: ${system_prompt}
UI_STATE:
${dom_summary}

USER: ${prompt}
ASSISTANT:`;

        try {
            const output = await this.pipe(input, {
                max_new_tokens: 256,
                temperature: 0.2, // Lower temp for more deterministic analysis
            });
            this.status = 'idle';
            const text = output[0].generated_text.split('ASSISTANT:')[1]?.trim() || "Analysis complete.";
            
            // Handle Autonomous Escalation
            if (text.includes('ESCALATE_TO_ARCHITECT')) {
                await this.escalate_to_architect(text, dom_summary);
            }
            
            return text;
        } catch (err) {
            this.status = 'error';
            console.error('🧠 [BrowserSpecialist] Inference failed:', err);
            return "ERROR: Local inference failed.";
        }
    }

    /**
     * Escalates a high-entropy state to the local computer models.
     */
    private async escalate_to_architect(reason: string, context: string) {
        console.warn('🧠 [BrowserSpecialist] HIGH ENTROPY DETECTED. Escalating to Computer Architect...');
        
        const { event_bus } = await import('./event_bus');
        event_bus.emit_log({
            source: 'System',
            text: `🚨 SENTINEL ALERT: High entropy detected in UI. Escalating to Architect core for remediation. Reason: ${reason.substring(0, 100)}...`,
            severity: 'error',
            metadata: {
                escalation_reason: reason,
                ui_context: context,
                type: 'NEURAL_HANDOFF'
            }
        });
    }

    get_status(): InferenceStatus {
        return this.status;
    }
}

export const browser_inference_service = new BrowserInferenceService();

// Metadata: [browser_inference]
