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

        console.log('👁️ [VisualBridge] Initializing bridge...');

        tadpole_os_socket.subscribe_custom_event(async (event) => {
            if (event.type === 'ui:inspect' && event.request_id && event.prompt) {
                this.handle_inspect_request(event.request_id, event.prompt);
            } else if (event.type === 'memory:save' && event.text) {
                const { local_memory_service } = await import('./local_memory');
                await local_memory_service.save(event.text, event.metadata || {});
            } else if (event.type === 'memory:search' && event.query && event.request_id) {
                const { local_memory_service } = await import('./local_memory');
                const results = await local_memory_service.search(event.query, event.limit);
                tadpole_os_socket.send_event({
                    type: 'memory:search_response',
                    request_id: event.request_id,
                    results
                });
            }
        });
    }

    async handle_inspect_request(request_id: string, prompt: string) {
        console.log(`👁️ [VisualBridge] Received inspection request ${request_id}: ${prompt}`);
        
        const specialist = use_browser_specialist_store.getState();
        const analysis = await specialist.analyze_dom(prompt);

        console.log(`👁️ [VisualBridge] Analysis complete. Sending response...`);

        // Send the response back via the socket
        // The backend expects a 'ui:inspect_response' event
        (tadpole_os_socket as any).send_event({
            type: 'ui:inspect_response',
            request_id,
            analysis
        });
    }
}

export const visual_monitor_bridge = new VisualMonitorBridge();
