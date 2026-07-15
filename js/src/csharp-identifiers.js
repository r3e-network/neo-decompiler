const CSHARP_RESERVED_KEYWORDS = new Set([
  "abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char",
  "checked", "class", "const", "continue", "decimal", "default", "delegate", "do",
  "double", "else", "enum", "event", "explicit", "extern", "false", "finally",
  "fixed", "float", "for", "foreach", "goto", "if", "implicit", "in", "int",
  "interface", "internal", "is", "lock", "long", "namespace", "new", "null",
  "object", "operator", "out", "override", "params", "private", "protected", "public",
  "readonly", "ref", "return", "sbyte", "sealed", "short", "sizeof", "stackalloc",
  "static", "string", "struct", "switch", "this", "throw", "true", "try", "typeof",
  "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "virtual", "void",
  "volatile", "while",
]);

const CSHARP_CONTEXTUAL_KEYWORDS = new Set([
  "add", "alias", "ascending", "async", "await", "by",
  "descending", "dynamic", "equals", "file", "from", "get", "global", "group",
  "init", "into", "join", "let", "nameof", "nint", "notnull", "nuint", "on",
  "orderby", "partial", "record", "remove", "required", "scoped", "select", "set",
  "unmanaged", "value", "when", "where", "with", "yield",
]);

const CSHARP_KEYWORDS = new Set([
  ...CSHARP_RESERVED_KEYWORDS,
  ...CSHARP_CONTEXTUAL_KEYWORDS,
]);

export function csharpIdentifier(name) {
  return CSHARP_KEYWORDS.has(name) ? `@${name}` : name;
}

export function isCSharpContextualKeyword(name) {
  return CSHARP_CONTEXTUAL_KEYWORDS.has(name);
}
