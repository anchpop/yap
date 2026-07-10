// Widget-side stand-in for yap-frontend's `@/lib/utils`, whose real module
// imports WASM at module level. The pure helpers are shared from the app's
// own lib/pure; playAudio/playTempAudio keep their signatures but fetch
// bytes through the MCP bridge (see ../audio.ts).
export * from "../../../../yap-frontend/src/lib/pure";
export { playAudio, playTempAudio } from "../audio";
export type { VoiceActorInfo } from "../../../../yap-frontend-rs/pkg";
