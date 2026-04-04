export function getPosterDataUrl(
  posterBytes: Uint8Array | undefined,
): string | null {
  if (!posterBytes) return null;

  try {
    const uint8Array = posterBytes;
    let binaryString = "";
    const chunkSize = 8192;
    for (let i = 0; i < uint8Array.length; i += chunkSize) {
      const chunk = uint8Array.subarray(i, i + chunkSize);
      binaryString += String.fromCharCode(...chunk);
    }
    return `data:image/webp;base64,${btoa(binaryString)}`;
  } catch (error) {
    console.error("Failed to convert poster bytes to data URL:", error);
    return null;
  }
}
