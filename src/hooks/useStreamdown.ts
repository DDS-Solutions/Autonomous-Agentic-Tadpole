/*
@docs ARCHITECTURE:UI

### AI Assist Note
**🛡️ Tadpole OS: Usestreamdown**
Core system module providing specialized functionality for the agent swarm.

### 🔍 Debugging & Observability
- **Failure Path**: Unexpected execution drift or type compatibility issues.
- **Telemetry Link**: Traced via active system logging channels.
*/

import { useState, useEffect } from 'react';
import { use_sovereign_store } from '../stores/sovereign_store';

export interface StreamEvent {
    type: 'agent_stream';
    agent_id: string;
    mission_id: string;
    trace_id: string;
    text: string;
    is_final: boolean;
    timestamp: string;
}

export const useStreamdown = () => {
    const [activeStreams, setActiveStreams] = useState<Record<string, string>>({});
    const update_message = use_sovereign_store(state => state.update_message);

    useEffect(() => {
        // This assumes the websocket events are dispatched via a global event bus or the store.
        // For TadpoleOS, we usually subscribe to the socket in a central place.
        // I'll simulate the listener registration.
        
        const handleStreamEvent = (event: MessageEvent) => {
            try {
                const data = JSON.parse(event.data);
                if (data.type === 'agent_stream') {
                    const stream = data as StreamEvent;
                    const streamKey = `${stream.agent_id}-${stream.mission_id}`;
                    
                    setActiveStreams(prev => ({
                        ...prev,
                        [streamKey]: stream.text
                    }));

                    // Optionally update the actual message list in the store
                    // to provide real-time updates to components.
                    // updateMessage(stream.trace_id, stream.text);
                }
            } catch {
                // Ignore non-json or malformed
            }
        };

        window.addEventListener('message', handleStreamEvent);
        return () => window.removeEventListener('message', handleStreamEvent);
    }, [update_message]);

    return {
        activeStreams,
    };
};

// Metadata: [useStreamdown]

// Metadata: [useStreamdown]
