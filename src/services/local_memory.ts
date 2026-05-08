/**
 * @docs ARCHITECTURE:UI-Services
 * 
 * ### AI Assist Note
 * **@docs ARCHITECTURE:Agent**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[local_memory]` in observability traces.
 */

/**
 * @docs ARCHITECTURE:Agent
 * 
 * ### AI Assist Note
 * **Local Memory Engine**: A private, browser-side vector database.
 * Uses IndexedDB (Dexie) for persistence and local embeddings for semantic search.
 * Implements **Zero-Trust Memory**: ensuring that sensitive mission data 
 * never leaves the user's browser during RAG operations.
 */

import Dexie, { type Table } from 'dexie';
import { browser_inference_service } from './browser_inference';

export interface MemoryEntry {
    id?: number;
    text: string;
    metadata: Record<string, unknown>;
    embedding: number[];
    timestamp: number;
}

class LocalMemoryDB extends Dexie {
    memories!: Table<MemoryEntry>;

    constructor() {
        super('TadpoleLocalMemory');
        this.version(1).stores({
            memories: '++id, timestamp' // Embedding is stored but not indexed directly by Dexie
        });
    }
}

const db = new LocalMemoryDB();

class LocalMemoryService {
    /**
     * Stores a new piece of information in local memory.
     * Automatically generates a vector embedding for semantic retrieval.
     */
    async save(text: string, metadata: Record<string, unknown> = {}) {
        console.debug('🧠 [LocalMemory] Archiving to local vector store...');
        const embedding = await browser_inference_service.get_embedding(text);
        
        await db.memories.add({
            text,
            metadata,
            embedding,
            timestamp: Date.now()
        });
        
        console.debug('🧠 [LocalMemory] Archive complete.');
    }

    /**
     * Performs a semantic search across local memories.
     */
    async search(query: string, limit: number = 5): Promise<string[]> {
        console.debug(`🧠 [LocalMemory] Searching for: ${query}`);
        const query_vector = await browser_inference_service.get_embedding(query);

        
        // 1. Fetch all memories (In a production app with thousands of entries, 
        // we'd use a more optimized vector index, but for local use this is fine).
        const all_memories = await db.memories.toArray();
        
        // 2. Rank by cosine similarity
        const ranked = all_memories
            .map(mem => ({
                text: mem.text,
                score: this.cosine_similarity(query_vector, mem.embedding)
            }))
            .sort((a, b) => b.score - a.score)
            .slice(0, limit);

        return ranked.map(r => `[Score: ${r.score.toFixed(2)}] ${r.text}`);
    }

    private cosine_similarity(a: number[], b: number[]): number {
        let dot_product = 0;
        let norm_a = 0;
        let norm_b = 0;
        for (let i = 0; i < a.length; i++) {
            dot_product += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
        }
        return dot_product / (Math.sqrt(norm_a) * Math.sqrt(norm_b));
    }
}

export const local_memory_service = new LocalMemoryService();

// Metadata: [local_memory]
