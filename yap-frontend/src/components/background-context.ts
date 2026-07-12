import { createContext, useContext } from "react";

export interface BackgroundContextType {
  bumpBackground: (multiplier?: number) => void;
}

export const BackgroundContext = createContext<BackgroundContextType | null>(
  null,
);

// When no BackgroundShader provider is mounted (tests, the MCP widget iframe,
// any provider-less harness), "bump the background if there is one" is the
// honest contract — so the hook no-ops instead of throwing.
const NOOP_BACKGROUND: BackgroundContextType = {
  bumpBackground: () => {},
};

export function useBackground(): BackgroundContextType {
  return useContext(BackgroundContext) ?? NOOP_BACKGROUND;
}
