import { useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";

/**
 * 只在悬停时滚动，平时用省略号静态展示。
 *
 * 之前连接卡片的名称和地址两行一直在滚动，侧栏看起来一直在动、
 * 也读不出完整的主机地址；现在静止时截断并保留悬浮提示，悬停才滚动。
 */
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
  const [hovered, setHovered] = useState(false);

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

  const scrolling = overflow && hovered;
  const style = {
    "--marquee-duration": `${Math.max(7, text.length * 0.32)}s`,
  } as CSSProperties;

  return (
    <div
      ref={containerRef}
      className={`marquee ${scrolling ? "is-scrolling" : ""} ${className}`}
      title={text}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <div className="marquee-track" style={style}>
        <span className="marquee-copy">
          <span ref={contentRef}>{text}</span>
        </span>
        {scrolling && (
          <span className="marquee-copy" aria-hidden="true">
            {text}
          </span>
        )}
      </div>
    </div>
  );
}
