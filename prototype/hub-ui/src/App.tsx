// PROTOTYPE — throwaway. Ticket 015 "Prototype the hub UI".
// Three structurally different takes on the same three screens, switchable via
// ?variant= (floating bar, ←/→ keys). Screens: repo browse, proposal review,
// environments. All fake data, no persistence, no mutations.
import { useEffect, useState } from "react";
import { Switcher } from "./Switcher";
import { Familiar } from "./variants/Familiar";
import { ChangeFirst } from "./variants/ChangeFirst";
import { Console } from "./variants/Console";

export type Screen = "repo" | "proposals" | "envs";
export type VariantProps = { screen: Screen; setScreen: (s: Screen) => void };

const variants = [
  { key: "a", name: "Familiar — GitHub-shaped", Component: Familiar },
  { key: "b", name: "Change-first — the log is home", Component: ChangeFirst },
  { key: "c", name: "Console — dense operator", Component: Console },
];

function readParam(key: string, fallback: string, allowed: string[]) {
  const v = new URLSearchParams(location.search).get(key) ?? fallback;
  return allowed.includes(v) ? v : fallback;
}

function writeParam(key: string, value: string) {
  const url = new URL(location.href);
  url.searchParams.set(key, value);
  history.replaceState(null, "", url);
}

export function App() {
  const [variant, setVariant] = useState(() =>
    readParam("variant", "a", variants.map((v) => v.key)),
  );
  const [screen, setScreen] = useState<Screen>(
    () => readParam("screen", "repo", ["repo", "proposals", "envs"]) as Screen,
  );

  const pick = (key: string) => {
    setVariant(key);
    writeParam("variant", key);
  };
  const pickScreen = (s: Screen) => {
    setScreen(s);
    writeParam("screen", s);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, [contenteditable]")) return;
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      const i = variants.findIndex((v) => v.key === variant);
      const next = (i + (e.key === "ArrowRight" ? 1 : variants.length - 1)) % variants.length;
      pick(variants[next].key);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [variant]);

  const current = variants.find((v) => v.key === variant)!;
  return (
    <>
      <current.Component screen={screen} setScreen={pickScreen} />
      <Switcher
        variants={variants.map((v) => ({ key: v.key, name: v.name }))}
        current={variant}
        onPick={pick}
      />
    </>
  );
}
