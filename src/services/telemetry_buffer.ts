/**
 * @docs ARCHITECTURE:Telemetry
 * @docs OPERATIONS_MANUAL:Tracing
 * 
 * ### AI Assist Note
 * **Telemetry Buffer Service**: Client-side IndexedDB storage engine for buffering 
 * real-time WebSocket telemetry events (`handoff`, `swarm_pulse`, `log`, `trace`).
 * Enables scrubbable time-travel replay across mission execution timelines.
 * Features 7-day rolling TTL auto-pruning to prevent IndexedDB storage quota exhaustion.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: IndexedDB quota exceeded or database connection failure.
 * - **Telemetry Link**: Search `[TelemetryBuffer]` in browser console logs.
 */

export interface BufferedTelemetryEvent {
    id?: number;
    mission_id: string;
    event_type: 'handoff' | 'swarm_pulse' | 'log' | 'trace';
    timestamp: number;
    payload: Record<string, unknown>;
    // Backward compatibility aliases
    missionId?: string;
    eventType?: 'handoff' | 'swarm_pulse' | 'log' | 'trace';
}

const DB_NAME = 'TadpoleTelemetryBuffer';
const DB_VERSION = 2; // Incremented for compound index support
const STORE_NAME = 'events';
const PRUNE_INTERVAL_MS = 5 * 60 * 1000; // Auto-prune at most once per 5 minutes

class TelemetryBufferService {
    private db_promise: Promise<IDBDatabase> | null = null;
    private last_prune_timestamp = 0;

    private async get_db(): Promise<IDBDatabase> {
        if (this.db_promise) return this.db_promise;

        this.db_promise = new Promise((resolve, reject) => {
            if (typeof indexedDB === 'undefined') {
                reject(new Error('IndexedDB is not available in this environment'));
                return;
            }
            const request = indexedDB.open(DB_NAME, DB_VERSION);

            request.onupgradeneeded = (event) => {
                const db = (event.target as IDBOpenDBRequest).result;
                if (!db.objectStoreNames.contains(STORE_NAME)) {
                    const store = db.createObjectStore(STORE_NAME, { keyPath: 'id', autoIncrement: true });
                    store.createIndex('mission_id', 'mission_id', { unique: false });
                    store.createIndex('timestamp', 'timestamp', { unique: false });
                    store.createIndex('mission_timestamp', ['mission_id', 'timestamp'], { unique: false });
                } else {
                    // SEC: Access transaction directly from request object to avoid browser flakiness during upgrade
                    const store = request.transaction?.objectStore(STORE_NAME);
                    if (store && !store.indexNames.contains('mission_timestamp')) {
                        store.createIndex('mission_timestamp', ['mission_id', 'timestamp'], { unique: false });
                    }
                }
            };

            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error);
        });

        return this.db_promise;
    }

    /** Automatically deletes telemetry events older than max_days to guard IndexedDB quota bounds. */
    public async prune_old_events(max_days = 7): Promise<void> {
        try {
            const db = await this.get_db();
            const cutoff = Date.now() - (max_days * 86400 * 1000);
            return new Promise((resolve, reject) => {
                const tx = db.transaction(STORE_NAME, 'readwrite');
                const store = tx.objectStore(STORE_NAME);
                if (store.indexNames.contains('timestamp')) {
                    const index = store.index('timestamp');
                    const range = IDBKeyRange.upperBound(cutoff);
                    const request = index.openCursor(range);
                    request.onsuccess = () => {
                        const cursor = request.result;
                        if (cursor) {
                            cursor.delete();
                            cursor.continue();
                        } else {
                            resolve();
                        }
                    };
                    request.onerror = () => reject(request.error);
                } else {
                    resolve();
                }
            });
        } catch (e) {
            console.warn('[TelemetryBuffer] Failed to prune old events:', e);
        }
    }

    public async append_event(mission_id: string, event_type: BufferedTelemetryEvent['event_type'], payload: Record<string, unknown>): Promise<void> {
        try {
            const db = await this.get_db();
            
            // Periodically check and prune events older than 7 days
            const now = Date.now();
            if (now - this.last_prune_timestamp > PRUNE_INTERVAL_MS) {
                this.last_prune_timestamp = now;
                void this.prune_old_events(7);
            }

            return new Promise((resolve, reject) => {
                const tx = db.transaction(STORE_NAME, 'readwrite');
                const store = tx.objectStore(STORE_NAME);

                const event: BufferedTelemetryEvent = {
                    mission_id: mission_id || 'global',
                    event_type,
                    timestamp: now,
                    payload,
                    missionId: mission_id || 'global',
                    eventType: event_type
                };

                store.add(event);

                tx.oncomplete = () => resolve();
                tx.onerror = () => reject(tx.error);
            });
        } catch (e) {
            console.warn('[TelemetryBuffer] Failed to append event to IndexedDB:', e);
        }
    }

    public async query_events(mission_id: string, start_time?: number, end_time?: number): Promise<BufferedTelemetryEvent[]> {
        try {
            const db = await this.get_db();
            return new Promise((resolve, reject) => {
                const tx = db.transaction(STORE_NAME, 'readonly');
                const store = tx.objectStore(STORE_NAME);

                // Use compound index ['mission_id', 'timestamp'] for zero-heap memory filtering
                if (store.indexNames.contains('mission_timestamp')) {
                    const index = store.index('mission_timestamp');
                    const range = IDBKeyRange.bound(
                        [mission_id, start_time || 0],
                        [mission_id, end_time || Number.MAX_SAFE_INTEGER]
                    );

                    const request = index.getAll(range);
                    request.onsuccess = () => resolve(request.result as BufferedTelemetryEvent[]);
                    request.onerror = () => reject(request.error);
                } else {
                    const index = store.index('mission_id');
                    const request = index.getAll(IDBKeyRange.only(mission_id));
                    request.onsuccess = () => {
                        let results = request.result as BufferedTelemetryEvent[];
                        if (start_time) results = results.filter(e => e.timestamp >= start_time);
                        if (end_time) results = results.filter(e => e.timestamp <= end_time);
                        resolve(results);
                    };
                    request.onerror = () => reject(request.error);
                }
            });
        } catch (e) {
            console.error('[TelemetryBuffer] Failed to query events from IndexedDB:', e);
            return [];
        }
    }

    public async clear_mission(mission_id: string): Promise<void> {
        try {
            const db = await this.get_db();
            return new Promise((resolve, reject) => {
                const tx = db.transaction(STORE_NAME, 'readwrite');
                const store = tx.objectStore(STORE_NAME);
                const index = store.index('mission_id');
                const request = index.openCursor(IDBKeyRange.only(mission_id));

                request.onsuccess = () => {
                    const cursor = request.result;
                    if (cursor) {
                        cursor.delete();
                        cursor.continue();
                    } else {
                        resolve();
                    }
                };

                request.onerror = () => reject(request.error);
                tx.oncomplete = () => resolve();
            });
        } catch (e) {
            console.warn('[TelemetryBuffer] Failed to clear mission events:', e);
        }
    }

    // Clean backward compatibility bindings without code duplication
    public appendEvent = this.append_event.bind(this);
    public queryEvents = this.query_events.bind(this);
    public clearMission = this.clear_mission.bind(this);
    public pruneOldEvents = this.prune_old_events.bind(this);
}

export const telemetry_buffer = new TelemetryBufferService();
export const telemetryBuffer = telemetry_buffer;
// Metadata: [TelemetryBuffer]
