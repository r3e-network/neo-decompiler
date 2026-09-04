export const MAX_NEF_FILE_BYTES = 0x10_0000;
export const MAX_MANIFEST_FILE_BYTES = 0xffff;

export function fileAdmissionError(nefFile, manifestFile) {
  if (nefFile.size > MAX_NEF_FILE_BYTES) {
    return `NEF file is ${nefFile.size} bytes; maximum is ${MAX_NEF_FILE_BYTES} bytes.`;
  }
  if (manifestFile && manifestFile.size > MAX_MANIFEST_FILE_BYTES) {
    return `Manifest file is ${manifestFile.size} bytes; maximum is ${MAX_MANIFEST_FILE_BYTES} bytes.`;
  }
  return undefined;
}

export async function readAdmittedFiles(nefFile, manifestFile) {
  const error = fileAdmissionError(nefFile, manifestFile);
  if (error) {
    throw new RangeError(error);
  }

  const nefBytes = new Uint8Array(await nefFile.arrayBuffer());
  const manifestJson = manifestFile ? await manifestFile.text() : undefined;
  return { nefBytes, manifestJson };
}
