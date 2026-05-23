/**
 * @docs ARCHITECTURE:UI
 * 
 * ### AI Assist Note
 * **GPUBackground Component**: High-performance WebGPU/WebGL2 dynamic background.
 * Uses a smooth dynamic gradient transition backdrop.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Shader initialization error or lack of WebGL/WebGPU support falling back to transparent background.
 * - **Telemetry Link**: Search `[GPUBackground]` in UI tracing.
 */

import React from 'react';
import { Shader, LinearGradient } from 'shaders/react';
import { THEME_COLORS } from '../../constants/theme';
import { use_settings_store } from '../../stores/settings_store';

const BACKDROP_THEMES: Record<string, { colorA: string, colorB: string }> = {
    cyan: { colorA: THEME_COLORS.DARK_BG, colorB: '#022c22' },    // Emerald/cyan tint (default)
    emerald: { colorA: THEME_COLORS.DARK_BG, colorB: '#064e3b' }, // Deep forest emerald
    purple: { colorA: THEME_COLORS.DARK_BG, colorB: '#1e1b4b' },  // Nebula purple/indigo
    slate: { colorA: THEME_COLORS.DARK_BG, colorB: '#0f172a' },   // Deep slate
    amber: { colorA: THEME_COLORS.DARK_BG, colorB: '#2d1500' },   // Sunset amber/copper
};

/**
 * GPUBackground
 * Renders a high-performance WebGPU/WebGL2 dynamic background.
 * Uses a smooth dynamic gradient transition backdrop.
 */
export const GPUBackground: React.FC = () => {
    const backdrop_theme = use_settings_store(s => s.settings.backdrop_theme || 'cyan');
    const colors = BACKDROP_THEMES[backdrop_theme] || BACKDROP_THEMES.cyan;

    return (
        <div className="absolute inset-0 w-full h-full -z-50 pointer-events-none select-none overflow-hidden">
            <Shader 
                className="w-full h-full"
                colorSpace="srgb"
                toneMapping="neutral"
            >
                {/* Smooth base gradient pulsing between dark zinc/slate and selected theme accent */}
                <LinearGradient 
                    colorA={colors.colorA} 
                    colorB={colors.colorB} 
                    angle={45} 
                />
            </Shader>
            
            {/* Overlay grid motif for texture depth, aligned with standard theme variables */}
            <div 
                className="absolute inset-0 opacity-[0.015] pointer-events-none"
                style={{
                    backgroundImage: 'radial-gradient(circle, #ffffff 1px, transparent 1px)',
                    backgroundSize: '24px 24px'
                }}
            />
        </div>
    );
};

export default GPUBackground;

// Metadata: [GPUBackground]
