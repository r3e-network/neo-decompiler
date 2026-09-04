// Encode untrusted metadata for comments and other single-line human output.
// Printable characters are preserved; invisible controls that can create a
// new physical line or reorder displayed text are rendered as ASCII escapes.
export function escapeVisibleText(value) {
  let escaped = "";
  for (const character of String(value)) {
    switch (character) {
      case "\0": escaped += "\\0"; break;
      case "\u0007": escaped += "\\a"; break;
      case "\u0008": escaped += "\\b"; break;
      case "\t": escaped += "\\t"; break;
      case "\n": escaped += "\\n"; break;
      case "\u000B": escaped += "\\v"; break;
      case "\u000C": escaped += "\\f"; break;
      case "\r": escaped += "\\r"; break;
      default: {
        const codePoint = character.codePointAt(0);
        if (
          codePoint <= 0x1f ||
          (codePoint >= 0x7f && codePoint <= 0x9f) ||
          (codePoint >= 0xd800 && codePoint <= 0xdfff) ||
          codePoint === 0x2028 ||
          codePoint === 0x2029 ||
          isBidiControl(character)
        ) {
          escaped += `\\u{${codePoint.toString(16).toUpperCase().padStart(4, "0")}}`;
        } else {
          escaped += character;
        }
      }
    }
  }
  return escaped;
}

export function isBidiControl(character) {
  const codePoint = character.codePointAt(0);
  return (
    codePoint === 0x061c ||
    codePoint === 0x200e ||
    codePoint === 0x200f ||
    (codePoint >= 0x202a && codePoint <= 0x202e) ||
    (codePoint >= 0x2066 && codePoint <= 0x2069)
  );
}

// Encode string content for a regular C# quoted literal. Unlike
// `escapeVisibleText`, these escapes are intended to be interpreted by the C#
// compiler, so the resulting runtime value is identical to the input while
// the generated source remains single-line and directionally inert.
export function escapeCSharpStringContent(value) {
  let escaped = "";
  for (const character of String(value)) {
    switch (character) {
      case "\0": escaped += "\\0"; break;
      case "\u0007": escaped += "\\a"; break;
      case "\u0008": escaped += "\\b"; break;
      case "\u000C": escaped += "\\f"; break;
      case "\n": escaped += "\\n"; break;
      case "\r": escaped += "\\r"; break;
      case "\t": escaped += "\\t"; break;
      case "\u000B": escaped += "\\v"; break;
      case '"': escaped += '\\"'; break;
      case "\\": escaped += "\\\\"; break;
      case "\u2028": escaped += "\\u2028"; break;
      case "\u2029": escaped += "\\u2029"; break;
      default: {
        const codePoint = character.codePointAt(0);
        if (
          codePoint <= 0x1f ||
          (codePoint >= 0x7f && codePoint <= 0x9f) ||
          (codePoint >= 0xd800 && codePoint <= 0xdfff) ||
          isBidiControl(character)
        ) {
          escaped += `\\u${codePoint.toString(16).toUpperCase().padStart(4, "0")}`;
        } else {
          escaped += character;
        }
      }
    }
  }
  return escaped;
}
