import React from 'react';

interface LD_Json_Props {
    data: object;
}

/**
 * LD_Json
 * Safely injects structured data into the DOM for SEO/GEO optimization.
 * Prevents XSS by using proper serialization and ensuring it stays within a script tag.
 */
export const LD_Json: React.FC<LD_Json_Props> = ({ data }) => {
    // Sanitize data by ensuring it's a valid object and serializing safely
    const json_string = React.useMemo(() => {
        try {
            return JSON.stringify(data).replace(/</g, '\\u003c').replace(/>/g, '\\u003e');
        } catch (e) {
            console.error('[LD_Json] Serialization failed:', e);
            return '{}';
        }
    }, [data]);

    return (
        <script 
            type="application/ld+json" 
            dangerouslySetInnerHTML={{ __html: json_string }} 
        />
    );
};
