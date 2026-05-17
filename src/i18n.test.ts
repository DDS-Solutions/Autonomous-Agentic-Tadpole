/**
 * @docs ARCHITECTURE:TestSuites
 * 
 * ### AI Assist Note
 * **Verification of the Internationalization Engine & Interpolation Safety.**
 * Validates dot-notation key resolution, HTML parameter escaping for XSS protection, 
 * legacy fallback warnings, and defensive fallback paths for missing keys.
 * 
 * ### 🔍 Debugging & Observability
 * - **Telemetry Link**: Search `[i18n.test]` in tracing logs.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { i18n } from './i18n';

describe('I18n Engine Hardening & XSS Validation', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  describe('Happy Path - Standard Namespaced Lookups', () => {
    it('should resolve standard dot-notation layout strings correctly', () => {
      const brand = i18n.t('layout.sidebar.brand_name');
      expect(brand).toBe('A-A-Tadpole-OS');
    });

    it('should resolve nested nav lookups correctly', () => {
      const workspaces = i18n.t('layout.nav.workspaces');
      expect(workspaces).toBe('Workspaces');
    });

    it('should perform standard parameter interpolation correctly without encoding alphanumeric text', () => {
      // workspaces.label_cluster_info has "{{dept}}" and "{{path}}" in layout.json
      const resolved = i18n.t('workspaces.label_cluster_info', {
        dept: 'Security',
        path: '/d/sec'
      });
      expect(resolved).toContain('Security');
      expect(resolved).toContain('/d/sec');
    });
  });

  describe('XSS Protection & Escaping Integrity', () => {
    it('should escape dangerous HTML characters to prevent template injection and XSS', () => {
      const resolved = i18n.t('workspaces.label_cluster_info', {
        dept: '<script>alert("XSS")</script>',
        path: 'javascript:void(0)'
      });

      // Assert that standard HTML tags are fully escaped
      expect(resolved).not.toContain('<script>');
      expect(resolved).toContain('&lt;script&gt;alert(&quot;XSS&quot;)&lt;/script&gt;');
    });

    it('should escape ampersands, quotes, and backticks', () => {
      const resolved = i18n.t('workspaces.label_cluster_info', {
        dept: '& " \' > <',
        path: 'test'
      });

      expect(resolved).toContain('&amp; &quot; &#039; &gt; &lt;');
    });
  });

  describe('Defensive Legacy Fallback Scanner', () => {
    it('should fall back to scanning nested modules when a flat legacy key is used', () => {
      const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const resolved = i18n.t('sidebar.brand_name');
      
      expect(resolved).toBe('A-A-Tadpole-OS');
      expect(consoleWarnSpy).toHaveBeenCalledTimes(1);
      expect(consoleWarnSpy.mock.calls[0][0]).toContain(
        '[I18n Warning] Legacy flat lookup used for "sidebar.brand_name". Please migrate to absolute dot-notation path: "layout.sidebar.brand_name"'
      );
    });

    it('should fall back to scanning nested modules for a flat key with double-nesting', () => {
      const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const resolved = i18n.t('nav.voice_interface_standups');
      
      expect(resolved).toBe('Voice Interface Standups');
      expect(consoleWarnSpy).toHaveBeenCalledTimes(1);
      expect(consoleWarnSpy.mock.calls[0][0]).toContain(
        '[I18n Warning] Legacy flat lookup used for "nav.voice_interface_standups". Please migrate to absolute dot-notation path: "layout.nav.voice_interface_standups"'
      );
    });
  });

  describe('Failure Paths & Edge Cases', () => {
    it('should return the literal key name if a key is not found in any module', () => {
      const resolved = i18n.t('invalid_namespace.non_existent_key');
      expect(resolved).toBe('invalid_namespace.non_existent_key');
    });

    it('should return default value if key is missing but defaultValue parameter is provided', () => {
      const resolved = i18n.t('missing.key.with.default', { defaultValue: 'Default Text' });
      expect(resolved).toBe('Default Text');
    });

    it('should return empty string for null or empty keys', () => {
      expect(i18n.t('')).toBe('');
      expect(i18n.t(null as any)).toBe('');
    });

    it('should return the raw JSON object if returnObjects is true', () => {
      const sidebarObj = i18n.t('layout.sidebar', { returnObjects: true });
      expect(sidebarObj).toBeTypeOf('object');
      expect(sidebarObj.brand_name).toBe('A-A-Tadpole-OS');
    });
  });
});
