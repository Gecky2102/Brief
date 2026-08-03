import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  markdown: string;
};

/// Il report arriva da un modello, quindi è testo non fidato: si rende come
/// Markdown senza abilitare l'HTML grezzo, così un documento malevolo non può
/// iniettare markup nella pagina.
export default function ReportView({ markdown }: Props) {
  return (
    <article className="brief-report mx-auto max-w-[46rem] pb-12 text-[13.5px] leading-[1.75]">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          h1: ({ children }) => (
            <h1 className="mb-6 border-b border-edge pb-4 text-[30px] font-semibold leading-tight tracking-tight">
              {children}
            </h1>
          ),
          h2: ({ children }) => (
            <h2 className="mb-3 mt-10 text-[19px] font-semibold tracking-tight">
              {children}
            </h2>
          ),
          h3: ({ children }) => (
            <h3 className="mb-1.5 mt-7 text-[15px] font-semibold text-ink">
              {children}
            </h3>
          ),
          h4: ({ children }) => (
            <h4 className="mb-1 mt-5 text-[13.5px] font-semibold text-ink-muted">
              {children}
            </h4>
          ),
          p: ({ children }) => <p className="my-3.5">{children}</p>,
          ul: ({ children }) => (
            <ul className="my-3 list-disc space-y-1.5 pl-5">{children}</ul>
          ),
          ol: ({ children }) => (
            <ol className="my-3 list-decimal space-y-1.5 pl-5">{children}</ol>
          ),
          li: ({ children }) => <li className="pl-1">{children}</li>,
          strong: ({ children }) => (
            <strong className="font-semibold">{children}</strong>
          ),
          blockquote: ({ children }) => (
            <blockquote className="my-4 border-l-2 border-accent pl-4 text-ink-muted">
              {children}
            </blockquote>
          ),
          hr: () => <hr className="my-8 border-edge" />,
          code: ({ children }) => (
            <code className="rounded bg-surface-raised px-1.5 py-0.5 text-[12px]">
              {children}
            </code>
          ),
          table: ({ children }) => (
            <div className="my-6 overflow-x-auto rounded-xl border border-edge">
              <table className="w-full border-collapse text-[12.5px]">
                {children}
              </table>
            </div>
          ),
          thead: ({ children }) => (
            <thead className="bg-surface-raised text-left">{children}</thead>
          ),
          th: ({ children }) => (
            <th className="border-b border-edge px-3.5 py-2.5 text-[11px] font-semibold uppercase tracking-wide text-ink-muted">
              {children}
            </th>
          ),
          td: ({ children }) => (
            <td className="border-b border-edge px-3.5 py-2.5 align-top leading-relaxed">
              {children}
            </td>
          ),
          tbody: ({ children }) => (
            <tbody className="[&>tr:last-child>td]:border-b-0">{children}</tbody>
          ),
        }}
      >
        {markdown}
      </ReactMarkdown>
    </article>
  );
}
