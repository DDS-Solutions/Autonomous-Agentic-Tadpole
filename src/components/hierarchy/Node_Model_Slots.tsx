/**
 * @docs ARCHITECTURE:Interface
 * 
 * ### AI Assist Note
 * **UI Component**: Tri-slot neural model selector for multi-inference agents. 
 * Facilitates hot-swapping active model slots and updating model configurations via the `model_store`.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: `active_model_slot` mismatch between UI LED and backend state, dropdown mount failure on window edge, or "Unknown Model" fallbacks during provider outages.
 * - **Telemetry Link**: Search for `[Node_Model_Slots]` or `Model_Badge` in UI logs.
 */

import React, { useRef, useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { i18n } from '../../i18n';
import { Tooltip } from '../ui';
import { Model_Badge } from '../Model_Badge';
import { use_dropdown_store } from '../../stores/dropdown_store';
import { use_model_store } from '../../stores/model_store';
import { resolve_friendly_model_name, resolve_technical_model_id } from '../../utils/model_utils';
import type { Agent } from '../../types';

interface Node_Model_Slots_Props {
    agent: Agent;
    on_model_change?: (agent_id: string, new_model: string) => void;
    on_model_2_change?: (agent_id: string, new_model: string) => void;
    on_model_3_change?: (agent_id: string, new_model: string) => void;
    on_update?: (agent_id: string, updates: Partial<Agent>) => void;
}

export const Node_Model_Slots: React.FC<Node_Model_Slots_Props> = ({
    agent,
    on_model_change,
    on_model_2_change,
    on_model_3_change,
    on_update
}) => {
    const toggle_dropdown = use_dropdown_store(s => s.toggle_dropdown);
    const close_dropdown = use_dropdown_store(s => s.close_dropdown);
    const is_model_1_open = use_dropdown_store(s => s.is_open(agent.id, 'model'));
    const is_model_2_open = use_dropdown_store(s => s.is_open(agent.id, 'model_2'));
    const is_model_3_open = use_dropdown_store(s => s.is_open(agent.id, 'model_3'));

    const available_models = use_model_store(s => s.models);

    const dropdown_states: Record<number, boolean> = {
        1: is_model_1_open,
        2: is_model_2_open,
        3: is_model_3_open
    };

    const handle_slot_change = (slot_idx: number, new_model: string) => {
        const handlers = [on_model_change, on_model_2_change, on_model_3_change];
        handlers[slot_idx - 1]?.(agent.id, new_model);
    };

    const ref1 = useRef<HTMLDivElement>(null);
    const ref2 = useRef<HTMLDivElement>(null);
    const ref3 = useRef<HTMLDivElement>(null);

    const [dropdown_pos, set_dropdown_pos] = useState<Record<number, { top: number, left: number }>>({});

    useEffect(() => {
        const slot_refs: Record<number, React.RefObject<HTMLDivElement | null>> = { 1: ref1, 2: ref2, 3: ref3 };
        const new_pos: Record<number, { top: number, left: number }> = {};
        [1, 2, 3].forEach((slot) => {
            const is_open = slot === 1 ? is_model_1_open : slot === 2 ? is_model_2_open : is_model_3_open;
            if (is_open && slot_refs[slot].current) {
                const rect = slot_refs[slot].current!.getBoundingClientRect();
                new_pos[slot] = {
                    top: rect.bottom,
                    left: Math.max(0, rect.left)
                };
            }
        });
        set_dropdown_pos(new_pos);
    }, [is_model_1_open, is_model_2_open, is_model_3_open]);

    const render_slot = (slot_idx: 1 | 2 | 3, raw_model: string | undefined, is_open: boolean) => {
        const is_active_slot = agent.active_model_slot === slot_idx || (slot_idx === 1 && !agent.active_model_slot && agent.status !== 'idle' && agent.status !== 'offline');

        const display_name = raw_model ? resolve_friendly_model_name(raw_model) || raw_model : undefined;
        const technical_id = raw_model ? resolve_technical_model_id(raw_model) || raw_model : undefined;

        const resolved_model_data = available_models.find(m => 
            (display_name && m.name.toLowerCase() === display_name.toLowerCase()) || 
            (technical_id && m.id.toLowerCase() === technical_id.toLowerCase()) ||
            (raw_model && (m.name.toLowerCase() === raw_model.toLowerCase() || m.id.toLowerCase() === raw_model.toLowerCase()))
        );

        const led_color =
            slot_idx === 1 ? 'bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]' :
                slot_idx === 2 ? 'bg-green-500 shadow-[0_0_8px_rgba(59,130,246,0.5)]' :
                    'bg-amber-500 shadow-[0_0_8px_rgba(245,158,11,0.5)]';

        return (
            <div ref={slot_idx === 1 ? ref1 : slot_idx === 2 ? ref2 : ref3} className="relative" key={`slot-${slot_idx}`} role="presentation" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
                <Tooltip content={i18n.t(`agent_card.tooltip_activate_${slot_idx === 1 ? 'primary' : slot_idx === 2 ? 'secondary' : 'tertiary'}`)} position="top">
                    <button
                        onClick={(e) => {
                            e.stopPropagation();
                            if (agent.active_model_slot !== slot_idx) {
                                on_update?.(agent.id, { active_model_slot: slot_idx });
                            }
                        }}
                        className="absolute -top-7 left-1/2 -translate-x-1/2 w-6 h-6 flex items-center justify-center cursor-pointer z-30 group/led"
                    >
                        <div className={`
                            w-1.5 h-1.5 rounded-full transition-all duration-300
                            ${is_active_slot ? `${led_color} scale-110` : 'bg-zinc-800 group-hover/led:bg-zinc-700'}
                        `} />
                    </button>
                </Tooltip>
                <Model_Badge
                    model={display_name || (slot_idx === 1 ? i18n.t('agent_card.label_unknown_model') : i18n.t('agent_card.label_add_model'))}
                    is_active={is_active_slot}
                    capabilities={resolved_model_data?.capabilities}
                    on_click={() => toggle_dropdown(agent.id, slot_idx === 1 ? 'model' : slot_idx === 2 ? 'model_2' : 'model_3')}
                />
                {is_open && createPortal(
                    <div className="fixed mt-1 w-56 bg-zinc-950 border border-zinc-800 rounded-xl shadow-2xl z-50 py-1.5 max-h-60 overflow-y-auto custom-scrollbar"
                         style={{ top: dropdown_pos[slot_idx]?.top || 0, left: dropdown_pos[slot_idx]?.left || 0 }}>
                        {available_models.map((m) => {
                            const is_current = display_name === m.name || technical_id === m.id;
                            return (
                                <button key={m.id} onClick={() => {
                                    handle_slot_change(slot_idx, m.name);
                                    close_dropdown();
                                }}
                                    className={`w-full text-left px-3 py-2 text-[10px] hover:bg-zinc-900 transition-colors flex items-center justify-between gap-2 ${is_current ? 'text-green-400 font-bold bg-green-500/5' : 'text-zinc-400'}`}>
                                    <span className="truncate">{m.name}</span>
                                    <div className="flex items-center gap-1 opacity-40">
                                        {m.capabilities?.supports_vision && <span title="Vision">👁️</span>}
                                        {m.capabilities?.supports_tools && <span title="Tools">🛠️</span>}
                                        {m.capabilities?.supports_reasoning && <span title="Reasoning">🧠</span>}
                                    </div>
                                </button>
                            );
                        })}
                    </div>,
                    document.body
                )}
            </div>
        );
    };

    const slot_models: Record<1 | 2 | 3, string | undefined> = {
        1: agent.model_config?.modelId || (agent.model_config as any)?.model || agent.model,
        2: agent.model_config2?.modelId || (agent.model_config2 as any)?.model || agent.model_2,
        3: agent.model_config3?.modelId || (agent.model_config3 as any)?.model || agent.model_3
    };

    return (
        <div className={`flex flex-col gap-1.5 border-t border-zinc-800 pt-2 relative ${is_model_1_open || is_model_2_open || is_model_3_open ? 'z-50' : 'z-20'}`}>
            <div className="flex items-center gap-1.5 overflow-visible">
                {([1, 2, 3] as const).map((slot_num) => (
                    render_slot(
                        slot_num, 
                        slot_models[slot_num], 
                        dropdown_states[slot_num]
                    )
                ))}
            </div>
        </div>
    );
};


// Metadata: [Node_Model_Slots]
