/**
 * SPARQL/Turtle Editor Component
 *
 * Professional code editor with:
 * - Syntax highlighting for SPARQL/Turtle/RDF
 * - Dark mode support
 * - Basic SPARQL validation
 * - Auto-completion
 */

import React, { useEffect, useMemo, useRef, useState } from 'react';
import Editor, { Monaco } from '@monaco-editor/react';
import { AlertCircle, CheckCircle } from 'lucide-react';

interface EditorWord {
  startColumn: number;
  endColumn: number;
}

interface EditorPosition {
  lineNumber: number;
  column: number;
}

interface CompletionItemLike {
  label: string;
  kind: number;
  insertText: string;
  insertTextRules?: number;
  documentation?: string;
  range: {
    startLineNumber: number;
    endLineNumber: number;
    startColumn: number;
    endColumn: number;
  };
}

interface EditorModelLike {
  getWordUntilPosition: (position: EditorPosition) => EditorWord;
}

export interface SPARQLEditorProps {
  value: string;
  onChange: (value: string) => void;
  height?: string | number;
  language?: 'sparql' | 'turtle' | 'rdf';
  placeholder?: string;
  readOnly?: boolean;
  showValidation?: boolean;
}

export function SPARQLEditor({
  value,
  onChange,
  height = 400,
  language = 'turtle',
  placeholder,
  readOnly = false,
  showValidation = true,
}: SPARQLEditorProps) {
  const editorRef = useRef<unknown>(null);
  const monacoRef = useRef<Monaco | null>(null);

  // Reactive dark mode detection
  const [isDark, setIsDark] = useState(
    () => document.documentElement.classList.contains('dark')
  );

  // Watch for theme changes
  useEffect(() => {
    const observer = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        if (mutation.attributeName === 'class') {
          setIsDark(document.documentElement.classList.contains('dark'));
        }
      });
    });

    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => observer.disconnect();
  }, []);

  // Validate SPARQL/Turtle syntax
  const validation = useMemo(() => {
    if (!showValidation || !value.trim()) {
      return { valid: true, errors: [] as string[] };
    }

    const errors: string[] = [];
    const lines = value.split('\n');

    // Basic Turtle validation
    if (language === 'turtle') {
      // Check for common syntax errors
      lines.forEach((line, idx) => {
        const trimmed = line.trim();

        // Skip empty lines and comments
        if (!trimmed || trimmed.startsWith('#')) return;

        // Check prefix declarations
        if (trimmed.startsWith('@prefix')) {
          if (!trimmed.includes('<') || !trimmed.includes('>') || !trimmed.endsWith('.')) {
            errors.push(`Line ${idx + 1}: Invalid @prefix declaration`);
          }
        }

        // Check for unclosed angle brackets
        const openBrackets = (trimmed.match(/</g) || []).length;
        const closeBrackets = (trimmed.match(/>/g) || []).length;
        if (openBrackets !== closeBrackets) {
          errors.push(`Line ${idx + 1}: Unclosed angle brackets`);
        }

        // Check for statements ending with period
        if (trimmed.includes('rdfs:') || trimmed.includes('owl:') || trimmed.includes('a ')) {
          const nextNonEmpty = lines.slice(idx + 1).find(l => l.trim());
          if (!trimmed.endsWith('.') && !trimmed.endsWith(';') && !trimmed.endsWith(',') &&
              nextNonEmpty && !nextNonEmpty.trim().startsWith('@prefix')) {
            // Only warn if not continuing on next line
            if (!nextNonEmpty.trim().match(/^(rdfs:|owl:|;|,|\.)/)) {
              errors.push(`Line ${idx + 1}: Missing statement terminator (. ; or ,)`);
            }
          }
        }
      });

      // Check for balanced quotes
      const doubleQuotes = (value.match(/"/g) || []).length;
      if (doubleQuotes % 2 !== 0) {
        errors.push('Unbalanced double quotes');
      }
    }

    // SPARQL-specific validation (sophisticated)
    if (language === 'sparql') {
      // Remove comments and normalize whitespace
      const cleanedValue = value
        .split('\n')
        .map(line => {
          const commentIndex = line.indexOf('#');
          return commentIndex >= 0 ? line.substring(0, commentIndex) : line;
        })
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim()
        .toUpperCase();

      // Skip validation if only PREFIX declarations or empty
      const withoutPrefixes = cleanedValue
        .replace(/PREFIX\s+\w+:\s*<[^>]+>/gi, '')
        .replace(/BASE\s+<[^>]+>/gi, '')
        .trim();

      if (withoutPrefixes.length > 0) {
        // SPARQL 1.1 Query types
        const queryTypes = ['SELECT', 'CONSTRUCT', 'ASK', 'DESCRIBE'];

        // SPARQL 1.1 Update operations
        const updateOps = [
          'INSERT DATA', 'DELETE DATA', 'DELETE WHERE', 'INSERT',
          'DELETE', 'LOAD', 'CLEAR', 'DROP', 'CREATE',
          'COPY', 'MOVE', 'ADD'
        ];

        const hasQueryType = queryTypes.some(type => cleanedValue.includes(type));
        const hasUpdateOp = updateOps.some(op => cleanedValue.includes(op));

        if (!hasQueryType && !hasUpdateOp) {
          errors.push(
            'Missing SPARQL operation (SELECT, CONSTRUCT, ASK, DESCRIBE, INSERT, DELETE, etc.)'
          );
        }

        // Check for balanced braces (only if we have content)
        const openBraces = (value.match(/\{/g) || []).length;
        const closeBraces = (value.match(/\}/g) || []).length;
        if (openBraces !== closeBraces) {
          errors.push('Unbalanced braces { }');
        }

        // Check for balanced parentheses
        const openParens = (value.match(/\(/g) || []).length;
        const closeParens = (value.match(/\)/g) || []).length;
        if (openParens !== closeParens) {
          errors.push('Unbalanced parentheses ( )');
        }

        // For SELECT queries, check for WHERE clause (unless it's SELECT * or SELECT DISTINCT *)
        if (cleanedValue.includes('SELECT') && !cleanedValue.includes('WHERE')) {
          // Allow simple SELECT queries without WHERE
          if (!cleanedValue.match(/SELECT\s+(\*|\?)/)) {
            errors.push('SELECT query typically requires WHERE clause');
          }
        }
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }, [value, language, showValidation]);

  // Configure Monaco on mount
  function handleEditorDidMount(
    editor: unknown,
    monaco: Monaco
  ) {
    editorRef.current = editor;
    monacoRef.current = monaco;

    // Register Turtle/SPARQL language if not already registered
    const languages = monaco.languages.getLanguages();

    if (!languages.find((languageDefinition: { id: string }) => languageDefinition.id === 'turtle')) {
      monaco.languages.register({ id: 'turtle' });

      // Define Turtle syntax highlighting
      monaco.languages.setMonarchTokensProvider('turtle', {
        keywords: [
          '@prefix', '@base', 'PREFIX', 'BASE',
          'a', 'true', 'false',
        ],
        typeKeywords: [
          'rdfs:Class', 'rdfs:Datatype', 'rdfs:Property',
          'owl:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty',
          'owl:Thing', 'owl:Nothing',
        ],
        operators: [';', ',', '.', '^', '|'],
        symbols: /[=><!~?:&|+*/^%-]+/,
        escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

        tokenizer: {
          root: [
            // Prefixes (use character class to avoid @ being interpreted as attribute reference)
            [/[@]prefix\b/, 'keyword'],
            [/[@]base\b/, 'keyword'],

            // URIs
            [/<[^>]+>/, 'type'],

            // Comments
            [/#.*$/, 'comment'],

            // Literals
            [/"([^"\\]|\\.)*$/, 'string.invalid'],
            [/"/, 'string', 'string_state'],

            // Numbers
            [/\d+/, 'number'],

            // Keywords
            [/[a-zA-Z][\w]*/, {
              cases: {
                '@keywords': 'keyword',
                '@typeKeywords': 'type',
                '@default': 'identifier',
              },
            }],
          ],

          string_state: [
            [/[^\\"]+/, 'string'],
            [/\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/, 'string.escape'],
            [/\\./, 'string.escape.invalid'],
            [/"/, 'string', '@pop'],
          ],
        },
      });
    }

    if (!languages.find((languageDefinition: { id: string }) => languageDefinition.id === 'sparql')) {
      monaco.languages.register({ id: 'sparql' });

      // Define SPARQL syntax highlighting
      monaco.languages.setMonarchTokensProvider('sparql', {
        keywords: [
          'SELECT', 'CONSTRUCT', 'ASK', 'DESCRIBE',
          'WHERE', 'FILTER', 'OPTIONAL', 'UNION',
          'GRAPH', 'FROM', 'DISTINCT', 'REDUCED',
          'ORDER', 'BY', 'LIMIT', 'OFFSET',
          'PREFIX', 'BASE', 'BIND', 'VALUES',
          'GROUP', 'HAVING', 'AS',
        ],
        functions: [
          'STR', 'LANG', 'DATATYPE', 'BOUND', 'sameTerm',
          'isIRI', 'isURI', 'isBlank', 'isLiteral',
          'REGEX', 'CONTAINS', 'STRSTARTS', 'STRENDS',
          'COUNT', 'SUM', 'MIN', 'MAX', 'AVG',
        ],
        operators: [';', ',', '.', '^', '|', '&&', '||', '!', '=', '!=', '<', '>', '<=', '>='],

        tokenizer: {
          root: [
            // Keywords (case-insensitive)
            [/[a-zA-Z]\w*/, {
              cases: {
                '@keywords': 'keyword',
                '@functions': 'predefined',
                '@default': 'identifier',
              },
            }],

            // URIs
            [/<[^>]+>/, 'type'],

            // Variables
            [/\?[\w]+/, 'variable'],
            [/\$[\w]+/, 'variable'],

            // Comments
            [/#.*$/, 'comment'],

            // Strings
            [/"([^"\\]|\\.)*$/, 'string.invalid'],
            [/"/, 'string', 'sparql_string'],
            [/'([^'\\]|\\.)*$/, 'string.invalid'],
            [/'/, 'string', 'sparql_single_string'],

            // Numbers
            [/\d+/, 'number'],
          ],

          sparql_string: [
            [/[^\\"]+/, 'string'],
            [/"/, 'string', '@pop'],
          ],

          sparql_single_string: [
            [/[^\\']+/, 'string'],
            [/'/, 'string', '@pop'],
          ],
        },
      });
    }

    // Set up autocompletion
    monaco.languages.registerCompletionItemProvider(language, {
      provideCompletionItems: (
        model: EditorModelLike,
        position: EditorPosition
      ) => {
        const word = model.getWordUntilPosition(position);
        const range = {
          startLineNumber: position.lineNumber,
          endLineNumber: position.lineNumber,
          startColumn: word.startColumn,
          endColumn: word.endColumn,
        };

        const suggestions: CompletionItemLike[] = [];

        if (language === 'turtle') {
          suggestions.push(
            {
              label: '@prefix',
              kind: monaco.languages.CompletionItemKind.Keyword,
              insertText: '@prefix ${1:prefix}: <${2:http://example.com/}> .',
              insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              documentation: 'Define a namespace prefix',
              range,
            },
            {
              label: 'rdfs:Class',
              kind: monaco.languages.CompletionItemKind.Class,
              insertText: 'a rdfs:Class ;\n    rdfs:label "${1:label}" .',
              insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              range,
            },
            {
              label: 'owl:Class',
              kind: monaco.languages.CompletionItemKind.Class,
              insertText: 'a owl:Class ;\n    rdfs:label "${1:label}" .',
              insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              range,
            },
          );
        } else if (language === 'sparql') {
          suggestions.push(
            {
              label: 'SELECT',
              kind: monaco.languages.CompletionItemKind.Keyword,
              insertText: 'SELECT ${1:?var}\nWHERE {\n    ${2:pattern}\n}',
              insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              range,
            },
            {
              label: 'FILTER',
              kind: monaco.languages.CompletionItemKind.Keyword,
              insertText: 'FILTER (${1:condition})',
              insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
              range,
            },
          );
        }

        return { suggestions };
      },
    });
  }

  return (
    <div className="relative border border-border rounded-lg overflow-hidden bg-muted">
      {/* Validation Status Bar */}
      {showValidation && value && (
        <div className={`px-3 py-1.5 border-b text-xs flex items-center gap-2 ${
          validation.valid
            ? 'bg-green-50 dark:bg-green-950/20 border-green-200 dark:border-green-900 text-green-700 dark:text-green-400'
            : 'bg-red-50 dark:bg-red-950/20 border-red-200 dark:border-red-900 text-red-700 dark:text-red-400'
        }`}>
          {validation.valid ? (
            <>
              <CheckCircle className="h-3.5 w-3.5" />
              <span>Valid {language.toUpperCase()} syntax</span>
            </>
          ) : (
            <>
              <AlertCircle className="h-3.5 w-3.5" />
              <span>{validation.errors.length} validation {validation.errors.length === 1 ? 'error' : 'errors'}</span>
            </>
          )}
        </div>
      )}

      {/* Monaco Editor */}
      <Editor
        height={height}
        language={language}
        value={value}
        onChange={(newValue) => onChange(newValue || '')}
        onMount={handleEditorDidMount}
        theme={isDark ? 'vs-dark' : 'vs'}
        options={{
          readOnly,
          minimap: { enabled: false },
          fontSize: 13,
          lineNumbers: 'on',
          glyphMargin: true,
          folding: true,
          lineDecorationsWidth: 10,
          lineNumbersMinChars: 3,
          scrollBeyondLastLine: false,
          automaticLayout: true,
          tabSize: 2,
          wordWrap: 'on',
          wrappingIndent: 'indent',
          suggest: {
            showKeywords: true,
            showSnippets: true,
          },
          quickSuggestions: {
            other: true,
            comments: false,
            strings: false,
          },
          ...(placeholder && !value ? {
            // Show placeholder using overlay
          } : {}),
        }}
        loading={
          <div className="flex items-center justify-center h-full">
            <div className="text-sm text-muted-foreground">Loading editor...</div>
          </div>
        }
      />

      {/* Validation Errors Panel */}
      {showValidation && !validation.valid && validation.errors.length > 0 && (
        <div className="border-t bg-red-50 dark:bg-red-950/10 p-3 max-h-32 overflow-y-auto">
          <div className="text-xs font-semibold text-red-700 dark:text-red-400 mb-2">
            Validation Errors:
          </div>
          <ul className="space-y-1">
            {validation.errors.map((error, idx) => (
              <li key={idx} className="text-xs text-red-600 dark:text-red-400 flex items-start gap-1.5">
                <span className="text-red-500 mt-0.5">•</span>
                <span>{error}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
