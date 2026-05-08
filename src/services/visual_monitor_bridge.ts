/**
 * @docs ARCHITECTURE:UI-Services
 * 
 * ### AI Assist Note
 * **@docs ARCHITECTURE:Agent**
 * Handles reactive state and high-fidelity user interactions.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: UI regression, hook desync, or API timeout.
 * - **Telemetry Link**: Search `[visual_monitor_bridge]` in observability traces.
 */

/**
 * @docs ARCHITECTURE:Agent
 * 
 * ### AI Assist Note
 * **Visual Monitor Bridge**: Connects the telemetry stream with the local 
 * inference specialist. Listens for backend inspection requests and 
 * handles the event-response loop.
 */

import { tadpole_os_socket } from './socket';
import { use_browser_specialist_store } from '../stores/browser_specialist_store';

class VisualMonitorBridge {
    private is_initialized = false;

    /**
     * Starts listening for inspection requests from the backend.
     */
    init() {
        if (this.is_initialized) return;
        this.is_initialized = true;

        console.debug('👁️ [VisualBridge] Initializing bridge...');

        tadpole_os_socket.subscribe_custom_event(async (event) => {
            if (event.type === 'ui:inspect' && event.request_id && event.prompt) {
                this.handle_inspect_request(String(event.request_id), String(event.prompt));
            } else if (event.type === 'memory:save' && event.text) {
                const { local_memory_service } = await import('./local_memory');
                await local_memory_service.save(String(event.text), (event.metadata as Record<string, unknown>) || {});
            } else if (event.type === 'memory:search' && event.query && event.request_id) {
                const { local_memory_service } = await import('./local_memory');
                const results = await local_memory_service.search(String(event.query), Number(event.limit) || undefined);
                tadpole_os_socket.send_event({
                    type: 'memory:search_response',
                    request_id: event.request_id,
                    results
                });
            }
        });
    }

    async handle_inspect_request(request_id: string, prompt: string) {
        console.debug(`👁️ [VisualBridge] Received inspection request ${request_id}: ${prompt}`);
        
        const specialist = use_browser_specialist_store.getState();
        const analysis = await specialist.analyze_dom(prompt);

        console.debug(`👁️ [VisualBridge] Analysis complete. Sending response...`);

        // Send the response back via the socket
        // The backend expects a 'ui:inspect_response' event
        tadpole_os_socket.send_event({
            type: 'ui:inspect_response',
            request_id,
            analysis
        });
    }
}

export const visual_monitor_bridge = new VisualMonitorBridge();

// Metadata: [visual_monitor_bridge]
