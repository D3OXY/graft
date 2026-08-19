// Floating prototype-only variant switcher — deliberately not part of the design.
export function Switcher({
  variants,
  current,
  onPick,
}: {
  variants: { key: string; name: string }[];
  current: string;
  onPick: (key: string) => void;
}) {
  if (import.meta.env.PROD) return null;
  const i = variants.findIndex((v) => v.key === current);
  const step = (d: number) => onPick(variants[(i + d + variants.length) % variants.length].key);
  return (
    <div className="fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-full bg-fuchsia-700 px-4 py-2 font-mono text-sm text-white shadow-xl">
      <button onClick={() => step(-1)} className="cursor-pointer px-1 hover:opacity-70" aria-label="previous variant">
        ←
      </button>
      <span>
        <b className="uppercase">{current}</b> — {variants[i].name}
      </span>
      <button onClick={() => step(1)} className="cursor-pointer px-1 hover:opacity-70" aria-label="next variant">
        →
      </button>
    </div>
  );
}
