type Runtime = import('@astrojs/cloudflare').Runtime<{
  DICTIONARY_KV: KVNamespace;
}>;

declare namespace App {
  interface Locals extends Runtime {}
}
