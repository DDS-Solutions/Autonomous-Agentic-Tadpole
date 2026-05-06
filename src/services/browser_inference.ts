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
        const input = `SYSTEM: You are a Browser Specialist Agent. Analyze the following UI state and answer the user query concisely.
UI_STATE:
${dom_summary}

USER: ${prompt}
ASSISTANT:`;

        try {
            const output = await this.pipe(input, {
                max_new_tokens: 128,
                temperature: 0.7,
            });
            this.status = 'idle';
            return output[0].generated_text.split('ASSISTANT:')[1]?.trim() || "Analysis complete.";
        } catch (err) {
            this.status = 'error';
            console.error('🧠 [BrowserSpecialist] Inference failed:', err);
            return "ERROR: Local inference failed.";
        }
    }

    get_status(): InferenceStatus {
        return this.status;
    }
}

export const browser_inference_service = new BrowserInferenceService();
