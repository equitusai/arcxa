declare module 'n3' {
  export class Parser {
    constructor(options?: { format?: string });
    parse(content: string, callback: (error: any, quad: any, prefixes?: any) => void): void;
  }

  export class Writer {
    constructor(options?: { format?: string; prefixes?: Record<string, string> });
    addQuad(quad: any): void;
    end(callback: (error: any, result: any) => void): void;
  }
}
