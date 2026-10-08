const BlobURLCache = new Map<string, { blobUrl: string; expiresAt: number }>();

export default async function BlobURLMaker(url: string): Promise<string | null> {
  if (!url) throw new Error("SpicyLyrics: BlobURLMaker: url Missing");
  const existingBlobURL = BlobURLCache.get(url);
  if (existingBlobURL) {
    const expiresAt = existingBlobURL.expiresAt;
    if (expiresAt < Date.now()) {
      BlobURLCache.delete(url);
    }
    return existingBlobURL.blobUrl;
  }
  try {
    const response = await fetch(url);
    if (!response.ok) {
      return null;
    }

    const blob = await response.blob();
    const blobUrl = URL.createObjectURL(blob);

    const expiresAt = Date.now() + 1000 * 60 * 60;
    BlobURLCache.set(url, {
      blobUrl,
      expiresAt,
    });
    return blobUrl;
  } catch (error) {
    console.error("Error fetching and converting to blob URL:", error);
    throw error;
  }
}
