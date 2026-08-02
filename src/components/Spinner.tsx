type Props = {
  label?: string;
  size?: "sm" | "md";
};

export default function Spinner({ label, size = "sm" }: Props) {
  const dimension = size === "sm" ? "h-3.5 w-3.5" : "h-5 w-5";
  return (
    <span className="inline-flex items-center gap-2">
      <svg
        className={`${dimension} animate-spin text-accent`}
        viewBox="0 0 24 24"
        fill="none"
        aria-hidden
      >
        <circle
          cx="12"
          cy="12"
          r="9"
          stroke="currentColor"
          strokeWidth="3"
          className="opacity-25"
        />
        <path
          d="M21 12a9 9 0 0 0-9-9"
          stroke="currentColor"
          strokeWidth="3"
          strokeLinecap="round"
        />
      </svg>
      {label && <span className="text-xs text-ink-muted">{label}</span>}
    </span>
  );
}
