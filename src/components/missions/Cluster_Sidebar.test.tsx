/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Tests the Cluster Sidebar's mission orchestration.** 
 * Verifies cluster selection, creation, budget editing (with debounce), and department switching.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Debounce timer leaks, budget input focus loss, or cluster selection state mismatch.
 * - **Telemetry Link**: Search `[Cluster_Sidebar.test]` in tracing logs.
 */

import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Cluster_Sidebar } from './Cluster_Sidebar';
import { i18n } from '../../i18n';

describe('Cluster_Sidebar Component', () => {
    const mock_props = {
        clusters: [
            {
                id: 'cl-1',
                name: 'Cluster One',
                department: 'Engineering' as any,
                theme: 'blue' as const,
                collaborators: ['a-1'],
                alpha_id: 'a-1',
                objective: 'Objective One',
                is_active: true,
                budget_usd: 50,
            }
        ],
        selected_cluster_id: 'cl-1',
        agents: [
            { id: 'a-1', name: 'Alpha Agent', role: 'Dev', theme_color: '#3b82f6' } as any
        ],
        on_select_cluster: vi.fn(),
        on_create_cluster: vi.fn(),
        on_delete_cluster: vi.fn(),
        on_toggle_active: vi.fn(),
        on_update_department: vi.fn(),
        on_update_budget: vi.fn(),
    };

    beforeEach(() => {
        vi.clearAllMocks();
        vi.useFakeTimers();
    });

    it('renders active clusters correctly', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        expect(screen.getByText('Cluster One')).toBeInTheDocument();
        expect(screen.getAllByText('Engineering').length).toBeGreaterThan(0);
        expect(screen.getByText(new RegExp(`1 ${i18n.t('missions.label_nodes')}`, 'i'))).toBeInTheDocument();
    });

    it('handles cluster selection', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        fireEvent.click(screen.getByText('Cluster One'));
        expect(mock_props.on_select_cluster).toHaveBeenCalledWith('cl-1');
    });

    it('opens and closes the create cluster modal', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        
        fireEvent.click(screen.getByText(new RegExp(i18n.t('missions.btn_new_mission'), 'i')));
        expect(screen.getByText(i18n.t('missions.header_create_cluster'))).toBeInTheDocument();
        
        // Modal is part of sidebar so no easy "close" without creating
    });

    it('creates a new cluster', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        fireEvent.click(screen.getByText(new RegExp(i18n.t('missions.btn_new_mission'), 'i')));
        
        const nameInput = screen.getByPlaceholderText(i18n.t('missions.placeholder_name'));
        fireEvent.change(nameInput, { target: { value: 'New Cluster' } });
        
        fireEvent.click(screen.getByText(i18n.t('missions.btn_create')));
        expect(mock_props.on_create_cluster).toHaveBeenCalledWith(expect.objectContaining({
            name: 'New Cluster',
            budget_usd: 1
        }));
    });

    it('deletes a cluster', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        const deleteBtn = screen.getByRole('button', { name: i18n.t('missions.tooltip_delete') });
        fireEvent.click(deleteBtn);
        expect(mock_props.on_delete_cluster).toHaveBeenCalledWith('cl-1');
    });

    it('toggles cluster active state', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        const toggleBtn = screen.getByRole('button', { name: i18n.t('missions.tooltip_deactivate') });
        fireEvent.click(toggleBtn);
        expect(mock_props.on_toggle_active).toHaveBeenCalledWith('cl-1');
    });

    it('updates cluster department', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        const select = screen.getByLabelText(i18n.t('missions.tooltip_reassign_dept'));
        fireEvent.change(select, { target: { value: 'Product' } });
        expect(mock_props.on_update_department).toHaveBeenCalledWith('cl-1', 'Product');
    });

    it('updates cluster budget with debounce', async () => {
        render(<Cluster_Sidebar {...mock_props} />);
        const budgetInput = screen.getByLabelText(i18n.t('missions.label_budget'));
        
        fireEvent.change(budgetInput, { target: { value: '100.50' } });
        
        // Should not be called immediately
        expect(mock_props.on_update_budget).not.toHaveBeenCalled();
        
        // Fast forward timers
        act(() => {
            vi.advanceTimersByTime(800);
        });
        
        expect(mock_props.on_update_budget).toHaveBeenCalledWith('cl-1', 100.5);
    });

    it('renders agent avatars in cluster card', () => {
        render(<Cluster_Sidebar {...mock_props} />);
        expect(screen.getByText('A')).toBeInTheDocument();
        // Check for alpha glow/dot if possible, but at least name initial
    });
});

// Metadata: [Cluster_Sidebar_test]

// Metadata: [Cluster_Sidebar_test]
