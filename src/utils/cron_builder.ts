/**
 * @docs ARCHITECTURE:Utilities
 * 
 * ### AI Assist Note
 * Pure cron expression generator and descriptor.
 * Enforces standard 5-field cron syntax matching `server-rs` scheduler requirements.
 * 
 * ### 🔍 Debugging & Observability
 * - Failure Path: Invalid date inputs or modulo overflow in cron fields.
 * - Telemetry Link: Traceable via `[CRON_BUILDER]` in client audit logs.
 */

export type Cron_Interval_Unit = 'minutes' | 'hours' | 'days' | 'months';
export type Cron_Timezone = 'local' | 'utc';

export interface Cron_Interval_Config {
    unit: Cron_Interval_Unit;
    interval: number;
    time?: string; // HH:mm
    day_of_month?: number; // 1-31
}

export interface Cron_Date_Config {
    date: string; // YYYY-MM-DD
    time: string; // HH:mm
    timezone: Cron_Timezone;
}

/**
 * Generates a valid standard 5-part cron expression for recurring intervals.
 */
export function synthesize_interval_cron(config: Cron_Interval_Config): string {
    const raw_interval = Math.max(1, Math.floor(config.interval || 1));
    const [h, m] = (config.time || '00:00').split(':').map(n => Math.max(0, parseInt(n, 10) || 0));
    const safe_minute = Math.min(59, m);
    const safe_hour = Math.min(23, h);

    switch (config.unit) {
        case 'minutes': {
            const step = Math.min(59, raw_interval);
            return step <= 1 ? '* * * * *' : `*/${step} * * * *`;
        }
        case 'hours': {
            const step = Math.min(23, raw_interval);
            return step <= 1 ? `${safe_minute} * * * *` : `${safe_minute} */${step} * * *`;
        }
        case 'days': {
            const step = Math.min(31, raw_interval);
            return step <= 1 
                ? `${safe_minute} ${safe_hour} * * *`
                : `${safe_minute} ${safe_hour} */${step} * *`;
        }
        case 'months': {
            const step = Math.min(12, raw_interval);
            const dom = Math.min(31, Math.max(1, config.day_of_month || 1));
            return step <= 1 
                ? `${safe_minute} ${safe_hour} ${dom} * *`
                : `${safe_minute} ${safe_hour} ${dom} */${step} *`;
        }
        default:
            return '0 * * * *';
    }
}

/**
 * Generates a standard 5-part cron expression for a specific calendar date and time.
 * If timezone is 'local', converts the local wall-clock date/time into UTC.
 */
export function synthesize_date_cron(config: Cron_Date_Config): string {
    if (!config.date || !config.time) {
        return '0 0 1 1 *';
    }

    const [year_s, month_s, day_s] = config.date.split('-');
    const [hour_s, min_s] = config.time.split(':');

    const year = parseInt(year_s, 10) || new Date().getFullYear();
    const month = parseInt(month_s, 10) || 1;
    const day = parseInt(day_s, 10) || 1;
    const hour = parseInt(hour_s, 10) || 0;
    const min = parseInt(min_s, 10) || 0;

    if (config.timezone === 'local') {
        const local_date = new Date(year, month - 1, day, hour, min, 0);
        const utc_min = local_date.getUTCMinutes();
        const utc_hour = local_date.getUTCHours();
        const utc_day = local_date.getUTCDate();
        const utc_month = local_date.getUTCMonth() + 1;
        return `${utc_min} ${utc_hour} ${utc_day} ${utc_month} *`;
    }

    const safe_min = Math.min(59, Math.max(0, min));
    const safe_hour = Math.min(23, Math.max(0, hour));
    const safe_day = Math.min(31, Math.max(1, day));
    const safe_month = Math.min(12, Math.max(1, month));

    return `${safe_min} ${safe_hour} ${safe_day} ${safe_month} *`;
}

/**
 * Returns a human-friendly English description of a standard cron expression.
 */
export function describe_cron(cron_expr: string, timezone: Cron_Timezone = 'utc'): string {
    const trimmed = (cron_expr || '').trim();
    if (!trimmed) return 'No schedule specified';
    const tz_label = timezone === 'local' ? '(Local)' : '(UTC)';

    // Handle shorthands
    if (trimmed === '@hourly') return `Runs every hour at minute 0 ${tz_label}`;
    if (trimmed === '@daily') return `Runs daily at 00:00 ${tz_label}`;
    if (trimmed === '@weekly') return `Runs weekly on Sunday at 00:00 ${tz_label}`;
    if (trimmed === '@monthly') return `Runs on the 1st of every month at 00:00 ${tz_label}`;

    // Every minute
    if (trimmed === '* * * * *') return 'Runs every minute';

    // Every N minutes
    const min_step_match = trimmed.match(/^\*\/(\d+)\s+\*\s+\*\s+\*\s+\*$/);
    if (min_step_match) {
        return `Runs every ${min_step_match[1]} minute(s)`;
    }

    // Every hour at minute M
    const hourly_match = trimmed.match(/^(\d+)\s+\*\s+\*\s+\*\s+\*$/);
    if (hourly_match) {
        return `Runs every hour at minute ${hourly_match[1]}`;
    }

    // Every N hours at minute M
    const hour_step_match = trimmed.match(/^(\d+)\s+\*\/(\d+)\s+\*\s+\*\s+\*$/);
    if (hour_step_match) {
        return `Runs every ${hour_step_match[2]} hour(s) at minute ${hour_step_match[1]}`;
    }

    // Daily at HH:mm
    const daily_match = trimmed.match(/^(\d+)\s+(\d+)\s+\*\s+\*\s+\*$/);
    if (daily_match) {
        const m = daily_match[1].padStart(2, '0');
        const h = daily_match[2].padStart(2, '0');
        return `Runs daily at ${h}:${m} ${tz_label}`;
    }

    // Every N days at HH:mm
    const days_step_match = trimmed.match(/^(\d+)\s+(\d+)\s+\*\/(\d+)\s+\*\s+\*$/);
    if (days_step_match) {
        const m = days_step_match[1].padStart(2, '0');
        const h = days_step_match[2].padStart(2, '0');
        return `Runs every ${days_step_match[3]} day(s) at ${h}:${m} ${tz_label}`;
    }

    // Monthly on day D at HH:mm
    const monthly_match = trimmed.match(/^(\d+)\s+(\d+)\s+(\d+)\s+\*\s+\*$/);
    if (monthly_match) {
        const m = monthly_match[1].padStart(2, '0');
        const h = monthly_match[2].padStart(2, '0');
        return `Runs monthly on day ${monthly_match[3]} at ${h}:${m} ${tz_label}`;
    }

    // Specific month & day at HH:mm
    const date_match = trimmed.match(/^(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+\*$/);
    if (date_match) {
        const m = date_match[1].padStart(2, '0');
        const h = date_match[2].padStart(2, '0');
        const day = date_match[3];
        const month = date_match[4];
        const month_names = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
        const month_name = month_names[parseInt(month, 10) - 1] || `Month ${month}`;
        return `Runs on ${month_name} ${day} at ${h}:${m} ${tz_label}`;
    }

    return `Schedule: ${trimmed}`;
}
