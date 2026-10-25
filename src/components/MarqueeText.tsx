import { useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";

export function MarqueeText({
  text,
  className = "",
}: {
  text: string;
  className?: string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLSpanElement>(null);
  const [overflow, setOverflow] = useState(false);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return;

    const measure = () => {
      const next = content.scrollWidth > container.clientWidth + 1;
      setOverflow(next);
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    observer.observe(content);
    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [text]);

  const style = {
    "--marquee-duration": `${Math.max(7, text.length * 0.32)}s`,
  } as CSSProperties;

  return (
    <div
      ref={containerRef}
      className={`marquee ${overflow ? "is-scrolling" : ""} ${className}`}
      title={text}
    >
      <div className="marquee-track" style={style}>
        <span className="marquee-copy">
          <span ref={contentRef}>{text}</span>
        </span>
        {overflow && (
          <span className="marquee-copy" aria-hidden="true">
            {text}
          </span>
        )}
      </div>
    </div>
  );
}
