/** The heading every settings card shares: one title, one plain sentence. */
export function Intro({ title, body }: { title: string; body: string }) {
  return (
    <div>
      <h2 className="text-base font-bold">{title}</h2>
      <p className="mt-1.5 max-w-[60ch] text-sm leading-relaxed text-muted-foreground">
        {body}
      </p>
    </div>
  );
}
