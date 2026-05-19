/**
 * @docs ARCHITECTURE:Infrastructure
 * 
 * ### AI Assist Note
 * **Root/Core**: Lifecycle-managed internationalization engine for Tadpole-OS. 
 * Orchestrates synchronous string lookups and dynamic object retrieval for department registries and system units.
 * 
 * ### 🔍 Debugging & Observability
 * - **Failure Path**: Key missing in `en.json` (returns the key string), circular object reference during `returnObjects: true`, or interpolation failure due to missing param keys.
 * - **Telemetry Link**: Search for `[I18n]` or `i18n.t` in browser logs.
 */

import common from './locales/common.json';
import system from './locales/system.json';
import layout from './locales/layout.json';
import swarm from './locales/swarm.json';
import telemetry from './locales/telemetry.json';
import security from './locales/security.json';
import skills from './locales/skills.json';
import provider from './locales/provider.json';
import config from './locales/config.json';
import jobs from './locales/jobs.json';
import templates from './locales/templates.json';
import docs from './locales/docs.json';
import workspaces from './locales/workspaces.json';
import voice from './locales/voice.json';
import chat from './locales/chat.json';

const en = {
  common,
  system,
  layout,
  swarm,
  telemetry,
  security,
  skills,
  provider,
  config,
  templates,
  scheduled_jobs: jobs,
  docs,
  workspaces,
  voice,
  chat
};

export type LocaleData = typeof en;


interface TOptions {
  returnObjects?: boolean;
}

/**
 * I18n
 * Core internationalization class for the Tadpole OS ecosystem.
 */
class I18n {
  private data: any = en;

  /**
   * t
   * Translates a key into a localized string or object.
   * 
   * @param key - The dot-notation path to the localization string (e.g. 'common.confirm')
   * @param params - Optional parameters for interpolation or {{returnObjects: true}} to get raw JSON chunks.
   * @returns The localized string or object.
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  t(key: string, params?: Record<string, string | number> | TOptions): any {
    if (!key) return '';

    const keys = key.split('.');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let result: any = this.data;
    let found = true;
    
    for (const k of keys) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      if (result && typeof result === 'object' && k in (result as Record<string, any>)) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        result = (result as Record<string, any>)[k];
      } else {
        found = false;
        break;
      }
    }

    // Defensive legacy fallback: scan modules for the first part of the dot-notation path
    if (!found) {
      const fallbackKeys = key.split('.');
      const firstKey = fallbackKeys[0];
      for (const [moduleName, moduleContent] of Object.entries(this.data)) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        if (moduleContent && typeof moduleContent === 'object' && firstKey in (moduleContent as Record<string, any>)) {
          // Try to resolve the full path starting from this module
          let fallbackResult = moduleContent;
          let fallbackFound = true;
          for (const k of fallbackKeys) {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            if (fallbackResult && typeof fallbackResult === 'object' && k in (fallbackResult as Record<string, any>)) {
              // eslint-disable-next-line @typescript-eslint/no-explicit-any
              fallbackResult = (fallbackResult as Record<string, any>)[k];
            } else {
              fallbackFound = false;
              break;
            }
          }
          if (fallbackFound) {
            result = fallbackResult;
            found = true;
            console.warn(`[I18n Warning] Legacy flat lookup used for "${key}". Please migrate to absolute dot-notation path: "${moduleName}.${key}"`);
            break;
          }
        }
      }
    }

    if (!found) {
      result = key;
    }

    const return_objects = params && (params as any).returnObjects;
    if (return_objects) {
      return result;
    }
    
    let text = found && typeof result === 'string' ? result : (params && (params as any).defaultValue) || key;
    
    if (params) {
      Object.entries(params as Record<string, string | number>).forEach(([k, v]) => {
        if (k !== 'defaultValue') {
          const valStr = typeof v === 'string' ? this.escapeHtml(v) : String(v);
          text = text.replace(new RegExp(`{{${k}}}`, 'g'), valStr);
        }
      });
    }
    
    return text;
  }

  /**
   * escapeHtml
   * Neutralizes dynamic XSS injection vectors in parameters.
   */
  private escapeHtml(str: string): string {
    return str.replace(/[&<>"']/g, (m) => {
      switch (m) {
        case '&': return '&amp;';
        case '<': return '&lt;';
        case '>': return '&gt;';
        case '"': return '&quot;';
        case "'": return '&#039;';
        default: return m;
      }
    });
  }
}

export const i18n = new I18n();

// Metadata: [i18n]

// Metadata: [i18n]
