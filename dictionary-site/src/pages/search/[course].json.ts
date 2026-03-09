import type { APIRoute } from 'astro';

export const GET: APIRoute = async ({ params, locals }) => {
  const kv = (locals as any).runtime.env.DICTIONARY_KV;
  const data = await kv.get(`search:${params.course}`);
  if (!data) {
    return new Response('Not found', { status: 404 });
  }
  return new Response(data, {
    headers: {
      'Content-Type': 'application/json',
      'Cache-Control': 'public, max-age=86400',
    },
  });
};
