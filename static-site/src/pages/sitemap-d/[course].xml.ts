import type { APIRoute } from 'astro';

export const GET: APIRoute = async ({ params, locals }) => {
  const courseSlug = params.course;
  const kv = (locals as any).runtime.env.DICTIONARY_KV;

  const lettersJson = await kv.get(`letters:${courseSlug}`);
  if (!lettersJson) {
    return new Response('Not found', { status: 404 });
  }

  const letters: { letter: string; count: number }[] = JSON.parse(lettersJson);

  // Collect all entry slugs from each letter
  const allEntries: string[] = [];
  const letterPages: string[] = [];

  for (const { letter } of letters) {
    letterPages.push(letter);
    const pagesJson = await kv.get(`letter:${courseSlug}:${letter}`);
    if (pagesJson) {
      const pages: { slug: string }[] = JSON.parse(pagesJson);
      for (const page of pages) {
        allEntries.push(page.slug);
      }
    }
  }

  const urls = [
    // Top words pages
    ...['top-1000', 'top-1000-words', 'top-1000-phrases'].map(
      (variant) => `<url>
  <loc>https://yap.town/d/${courseSlug}/${variant}/</loc>
  <priority>0.6</priority>
</url>`
    ),
    // Letter index pages
    ...letterPages.map(
      (letter) => `<url>
  <loc>https://yap.town/d/${courseSlug}/letter/${encodeURIComponent(letter)}/</loc>
  <priority>0.5</priority>
</url>`
    ),
    // Individual entry pages
    ...allEntries.map(
      (slug) => `<url>
  <loc>https://yap.town/d/${courseSlug}/${encodeURIComponent(slug)}/</loc>
  <priority>0.4</priority>
</url>`
    ),
  ];

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${urls.join('\n')}
</urlset>`;

  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml' },
  });
};
