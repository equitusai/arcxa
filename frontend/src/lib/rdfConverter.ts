/**
 * RDF Format Converter
 *
 * Utilities for detecting and converting between RDF formats
 * Supports: RDF/XML, Turtle, N-Triples, N-Quads
 */

import { Parser, Writer } from 'n3';

export type RDFFormat = 'turtle' | 'rdfxml' | 'ntriples' | 'nquads' | 'unknown';

/**
 * Detect RDF format from content
 */
export function detectRDFFormat(content: string): RDFFormat {
  const trimmed = content.trim();

  // Check for RDF/XML
  if (trimmed.startsWith('<?xml') ||
      trimmed.includes('<rdf:RDF') ||
      trimmed.includes('<RDF') ||
      trimmed.match(/<rdf:RDF[^>]*>/i)) {
    return 'rdfxml';
  }

  // Check for Turtle
  if (trimmed.includes('@prefix') ||
      trimmed.includes('@base') ||
      trimmed.match(/PREFIX\s+\w+:/i)) {
    return 'turtle';
  }

  // Check for N-Triples (simple triple pattern)
  if (trimmed.match(/^<[^>]+>\s+<[^>]+>\s+[<"].*[">]\s*\./m)) {
    return 'ntriples';
  }

  // Check for N-Quads (quad pattern)
  if (trimmed.match(/^<[^>]+>\s+<[^>]+>\s+[<"].*[">]\s+<[^>]+>\s*\./m)) {
    return 'nquads';
  }

  return 'unknown';
}

/**
 * Convert RDF content to Turtle format
 */
export async function convertToTurtle(
  content: string,
  sourceFormat?: RDFFormat
): Promise<{ success: true; turtle: string } | { success: false; error: string }> {
  try {
    // Auto-detect format if not provided
    const format = sourceFormat || detectRDFFormat(content);

    // If already Turtle, return as-is
    if (format === 'turtle') {
      return { success: true, turtle: content };
    }

    // Parse the input format
    const parser = new Parser({
      format: format === 'rdfxml' ? 'application/rdf+xml' : format,
    });

    const quads: any[] = [];

    // Parse to quads
    await new Promise<void>((resolve, reject) => {
      parser.parse(content, (error: any, quad: any, prefixes: any) => {
        if (error) {
          reject(error);
        } else if (quad) {
          quads.push(quad);
        } else {
          // End of parsing
          resolve();
        }
      });
    });

    if (quads.length === 0) {
      return {
        success: false,
        error: 'No RDF triples found in content',
      };
    }

    // Write as Turtle
    const writer = new Writer({
      format: 'text/turtle',
      prefixes: {
        rdf: 'http://www.w3.org/1999/02/22-rdf-syntax-ns#',
        rdfs: 'http://www.w3.org/2000/01/rdf-schema#',
        owl: 'http://www.w3.org/2002/07/owl#',
        xsd: 'http://www.w3.org/2001/XMLSchema#',
        dc: 'http://purl.org/dc/elements/1.1/',
      },
    });

    // Add all quads to writer
    quads.forEach(quad => writer.addQuad(quad));

    // Get Turtle output
    const turtle = await new Promise<string>((resolve, reject) => {
      writer.end((error: any, result: any) => {
        if (error) {
          reject(error);
        } else {
          resolve(result);
        }
      });
    });

    return { success: true, turtle };

  } catch (error: any) {
    return {
      success: false,
      error: error.message || 'Failed to convert RDF format',
    };
  }
}

/**
 * Validate RDF content
 */
export async function validateRDF(
  content: string,
  format?: RDFFormat
): Promise<{ valid: true; tripleCount: number } | { valid: false; error: string }> {
  try {
    const detectedFormat = format || detectRDFFormat(content);

    if (detectedFormat === 'unknown') {
      return {
        valid: false,
        error: 'Could not detect RDF format. Supported formats: Turtle, RDF/XML, N-Triples, N-Quads',
      };
    }

    const parser = new Parser({
      format: detectedFormat === 'rdfxml' ? 'application/rdf+xml' : detectedFormat,
    });

    let tripleCount = 0;

    await new Promise<void>((resolve, reject) => {
      parser.parse(content, (error: any, quad: any) => {
        if (error) {
          reject(error);
        } else if (quad) {
          tripleCount++;
        } else {
          resolve();
        }
      });
    });

    return { valid: true, tripleCount };

  } catch (error: any) {
    return {
      valid: false,
      error: error.message || 'Invalid RDF syntax',
    };
  }
}

/**
 * Get format display name
 */
export function getFormatDisplayName(format: RDFFormat): string {
  switch (format) {
    case 'turtle':
      return 'Turtle';
    case 'rdfxml':
      return 'RDF/XML';
    case 'ntriples':
      return 'N-Triples';
    case 'nquads':
      return 'N-Quads';
    default:
      return 'Unknown';
  }
}

/**
 * Get format description
 */
export function getFormatDescription(format: RDFFormat): string {
  switch (format) {
    case 'turtle':
      return 'Terse RDF Triple Language - human-readable format';
    case 'rdfxml':
      return 'RDF/XML - XML-based RDF serialization';
    case 'ntriples':
      return 'N-Triples - line-based plain text format';
    case 'nquads':
      return 'N-Quads - extension of N-Triples for named graphs';
    default:
      return 'Unknown RDF format';
  }
}
