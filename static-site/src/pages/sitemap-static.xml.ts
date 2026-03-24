import type { APIRoute } from 'astro';

export const GET: APIRoute = async ({ locals }) => {
  const kv = (locals as any).runtime.env.DICTIONARY_KV;
  const coursesJson = await kv.get('courses');
  const courses = coursesJson ? JSON.parse(coursesJson) : [];

  const urls = [
    { loc: 'https://yap.town/', priority: '1.0' },
    { loc: 'https://yap.town/d/', priority: '0.8' },
    { loc: 'https://yap.town/blog/', priority: '0.7' },
    // Blog posts
    {
      loc: 'https://yap.town/blog/unexpected-application-of-ml-theory-to-language-learning/',
      priority: '0.6',
    },
    // Per-course index pages
    ...courses.map((c: any) => ({
      loc: `https://yap.town/d/${c.slug}/`,
      priority: '0.7',
    })),
  ];

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls
  .map(
    (u) => `<url>
  <loc>${u.loc}</loc>
  <priority>${u.priority}</priority>
</url>`
  )
  .join('\n')}
</urlset>`;

  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml' },
  });
};
