export class NeoDecompilerError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = this.constructor.name;
    this.details = details;
  }
}

export class NefParseError extends NeoDecompilerError {}

export class DisassemblyError extends NeoDecompilerError {}

export class ManifestParseError extends NeoDecompilerError {}
