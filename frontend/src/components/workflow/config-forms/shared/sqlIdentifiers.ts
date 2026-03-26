function cleanIdentifierSegment(segment: string): string {
  return segment.trim().replace(/^[`"]+|[`"]+$/g, '');
}

function quoteDatabricksSegment(segment: string): string {
  return `\`${cleanIdentifierSegment(segment).replace(/[`"]/g, '')}\``;
}

function quoteOracleSegment(segment: string): string {
  const cleaned = cleanIdentifierSegment(segment);
  const normalized = /^[A-Za-z][A-Za-z0-9_$#]*$/.test(cleaned)
    ? cleaned.toUpperCase()
    : cleaned;

  return `"${normalized.replace(/"/g, '""')}"`;
}

function quotePostgreSqlSegment(segment: string): string {
  const cleaned = cleanIdentifierSegment(segment).replace(/""/g, '"');
  return `"${cleaned.replace(/"/g, '""')}"`;
}

export function quoteDatasourceIdentifier(
  identifier: string,
  sourceType?: string,
  options?: { qualified?: boolean }
): string {
  const qualified = options?.qualified ?? false;
  const segments = qualified ? identifier.split('.') : [identifier];
  const normalizedSegments = segments.map(cleanIdentifierSegment).filter(Boolean);

  if (normalizedSegments.length === 0) {
    return identifier;
  }

  if (sourceType === 'Databricks') {
    return normalizedSegments.map(quoteDatabricksSegment).join('.');
  }

  if (sourceType === 'Oracle') {
    return normalizedSegments.map(quoteOracleSegment).join('.');
  }

  if (sourceType === 'PostgreSQL') {
    return normalizedSegments.map(quotePostgreSqlSegment).join('.');
  }

  return normalizedSegments.join('.');
}

export function buildDatasourcePreviewQuery(
  tableName: string,
  selectedColumns?: string[],
  sourceType?: string
): string {
  const projection =
    selectedColumns && selectedColumns.length > 0
      ? selectedColumns
          .map((column) => quoteDatasourceIdentifier(column, sourceType))
          .join(', ')
      : '*';
  const quotedTable = quoteDatasourceIdentifier(tableName, sourceType, { qualified: true });
  return `SELECT ${projection} FROM ${quotedTable}`;
}
