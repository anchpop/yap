import type { APIRoute } from 'astro';

export const GET: APIRoute = async ({ locals }) => {
  const kv = (locals as any).runtime.env.DICTIONARY_KV;
  const coursesJson = await kv.get('courses');
  const courses = coursesJson ? JSON.parse(coursesJson) : [];

  const sitemaps = [
    `<sitemap><loc>https://yap.town/sitemap-static.xml</loc></sitemap>`,
    ...courses.map(
      (c: any) =>
        `<sitemap><loc>https://yap.town/sitemap-d/${c.slug}.xml</loc></sitemap>`
    ),
  ];

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${sitemaps.join('\n')}
</sitemapindex>`;

  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml' },
  });
};
