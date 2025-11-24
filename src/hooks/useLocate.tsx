import { waitForAllInnerImages } from "@/utils";
import React, { useCallback, useLayoutEffect, useRef } from "react";
import { createRoot } from "react-dom/client";

export function useLocate() {
  const measureRoot = document.getElementById("measure-root");
  if (!measureRoot) throw new Error("#measure-root missing");

  const locate = useCallback(
    (
      element: React.ReactNode,
      width: number,
      targetId?: string
    ): Promise<{ x: number; y: number; width: number; height: number }> => {
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
          const refOuter = useRef<HTMLDivElement>(null);

          useLayoutEffect(() => {
            const measure = () => {
              if (refOuter.current) {
                const rectOuter = refOuter.current.getBoundingClientRect();

                if (!targetId) {
                  resolve({
                    x: 0,
                    y: 0,
                    width: rectOuter.width,
                    height: rectOuter.height,
                  });
                }

                const innerElement = refOuter.current.querySelector(
                  `#${targetId}`
                )!;
                const rectInner = innerElement.getBoundingClientRect();

                resolve({
                  x: rectInner.left - rectOuter.left,
                  y: rectInner.top - rectOuter.top,
                  width: rectInner.width,
                  height: rectInner.height,
                });

                // Cleanup
                root.unmount();
                container.remove();
              }
            };

            // Let the browser lay out the element fully
            waitForAllInnerImages(refOuter.current!).then(() =>
              requestAnimationFrame(() => requestAnimationFrame(measure))
            );
          }, []);

          return (
            <div ref={refOuter} style={{ width }}>
              {element}
            </div>
          );
        };

        root.render(<Wrapper />);
      });
    },
    [measureRoot]
  );

  return { locate };
}
