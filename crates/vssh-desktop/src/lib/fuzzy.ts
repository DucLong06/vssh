/** Lower case, no spaces, no accents: `Kết nối` is found by `ket noi`. */
const fold = (text: string) =>
  text
    .normalize('NFD')
    .replace(/\p{M}|\s/gu, '')
    .replace(/đ/gi, 'd')
    .toLowerCase()

/**
 * Whether the letters of `query` appear in `text` in order. `null` if not;
 * otherwise how loose the match is: the letters skipped before and between
 * the ones that matched, so 0 is a prefix and lower is better.
 */
export function fuzzy(query: string, text: string): number | null {
  const hay = fold(text)
  let skipped = 0
  let at = -1
  for (const letter of fold(query)) {
    const next = hay.indexOf(letter, at + 1)
    if (next < 0) return null
    skipped += next - at - 1
    at = next
  }
  return skipped
}
