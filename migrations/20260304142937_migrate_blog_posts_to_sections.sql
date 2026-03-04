-- Add migration script here
ALTER TABLE blog_posts ADD COLUMN sections JSONB NOT NULL DEFAULT '[]'::jsonb;

UPDATE blog_posts
SET sections = jsonb_build_array(
    jsonb_build_object(
        'type', 'markdown',
        'content', content
    )
)
WHERE content IS NOT NULL AND content != '';

ALTER TABLE blog_posts DROP COLUMN content;