import React, { useCallback, useLayoutEffect, useRef } from "react";
import { createRoot } from "react-dom/client";

export function useMeasure() {
  const measureRoot = document.getElementById("measure-root");
  if (!measureRoot) throw new Error("#measure-root missing");

  const measure = useCallback(
    (
      element: React.ReactNode,
      width: number
    ): Promise<{ width: number; height: number }> => {
      return new Promise((resolve) => {
        // Create offscreen container
        const container = document.createElement("div");
        container.style.position = "absolute";
        container.style.top = "0";
        container.style.left = "0";
        container.style.visibility = "hidden";
        container.style.pointerEvents = "none";

        measureRoot.appendChild(container);

        // Create a React root in this container
        const root = createRoot(container);

        const Wrapper = () => {
          const ref = useRef<HTMLDivElement>(null);

          useLayoutEffect(() => {
            // Let the browser lay out the element fully
            requestAnimationFrame(() => {
              requestAnimationFrame(() => {
                if (ref.current) {
                  const rect = ref.current.getBoundingClientRect();

                  resolve({
                    width: rect.width,
                    height: rect.height,
                  });

                  // Cleanup
                  root.unmount();
                  container.remove();
                }
              });
            });
          }, []);

          return (
            <div ref={ref} style={{ width }}>
              {element}
            </div>
          );
        };

        root.render(<Wrapper />);
      });
    },
    [measureRoot]
  );

  return { measure };
}
