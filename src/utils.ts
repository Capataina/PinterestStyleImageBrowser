export function getImageSize(
  src: string
): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () =>
      resolve({
        width: img.naturalWidth,
        height: img.naturalHeight,
      });
    img.src = src;
    img.onerror = reject;
  });
}

export function waitForAllInnerImages(element: HTMLElement): Promise<void> {
  return new Promise((resolve) => {
    const images = Array.from(element.getElementsByTagName("img"));

    if (images.length == 0) resolve();

    let remaining = 0;

    const cleanup = () => {
      for (const img of images) {
        img.removeEventListener("load", onDone);
        img.removeEventListener("error", onDone);
      }
    };

    const onDone = () => {
      remaining--;
      if (remaining <= 0) {
        cleanup();
        resolve();
      }
    };

    const pending = images.filter(
      (img) => !img.complete || img.naturalWidth == 0
    );

    if (pending.length == 0) resolve();

    remaining = pending.length;

    pending.forEach((img) => {
      img.addEventListener("load", onDone);
      img.addEventListener("error", onDone);
    });
  });
}
