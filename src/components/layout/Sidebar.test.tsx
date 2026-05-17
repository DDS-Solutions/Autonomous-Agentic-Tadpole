/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Integration tests for the primary navigation sidebar.** 
 * Ensures all core operational links (Hierarchy, Missions, Oversight) are present and correctly highlighted. 
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Collision between sidebar z-index and modal overlays or failure to persist the 'Node Certified' status across navigation events.
 * - **Telemetry Link**: Search `[Sidebar.test]` in tracing logs.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { Sidebar } from './Sidebar';

vi.mock('../ui', () => ({
    Tooltip: ({ children, content }: any) => <div title={content}>{children}</div>
}));

describe('Sidebar Component', () => {
    const mockNavItemClass = ({ isActive }: { isActive: boolean }) => 
        isActive ? 'active-class' : 'inactive-class';

    it('renders navigation links and core sections', () => {
        render(
            <MemoryRouter>
                <Sidebar nav_item_class={mockNavItemClass} />
            </MemoryRouter>
        );

        expect(screen.getByText('A-A-Tadpole-OS')).toBeInTheDocument();
        expect(screen.getByText('Operations')).toBeInTheDocument();
        expect(screen.getByText('Hierarchy')).toBeInTheDocument();
        expect(screen.getByText('Missions')).toBeInTheDocument();
        expect(screen.getByText('Scheduled Jobs')).toBeInTheDocument();
        expect(screen.getByText('Oversight')).toBeInTheDocument();
        expect(screen.getByText('System Docs')).toBeInTheDocument();
        expect(screen.getByText('Settings')).toBeInTheDocument();
    });

    it('displays the certification badge', () => {
        render(
            <MemoryRouter>
                <Sidebar nav_item_class={mockNavItemClass} />
            </MemoryRouter>
        );

        expect(screen.getByText('Node Certified')).toBeInTheDocument();
        expect(screen.getByText('Sovereign Cluster v2.4')).toBeInTheDocument();
    });
});


// Metadata: [Sidebar_test]
