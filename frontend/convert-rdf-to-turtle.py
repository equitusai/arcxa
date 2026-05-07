#!/usr/bin/env python3
"""
Convert RDF/XML ontology to Turtle format
"""

import sys

try:
    from rdflib import Graph
except ImportError:
    print("Error: rdflib not installed. Install with: pip install rdflib")
    sys.exit(1)

if len(sys.argv) < 2:
    print("Usage: python convert-rdf-to-turtle.py <input.rdf> [output.ttl]")
    sys.exit(1)

input_file = sys.argv[1]
output_file = sys.argv[2] if len(sys.argv) > 2 else input_file.replace('.rdf', '.ttl')

try:
    # Parse RDF/XML
    print(f"Parsing {input_file} (RDF/XML format)...")
    g = Graph()
    g.parse(input_file, format='xml')

    # Serialize to Turtle
    print(f"Converting to Turtle format...")
    turtle_content = g.serialize(format='turtle')

    # Write output
    print(f"Writing to {output_file}...")
    with open(output_file, 'w') as f:
        f.write(turtle_content)

    print(f"✅ Success! Converted {len(g)} triples")
    print(f"   Output: {output_file}")

except Exception as e:
    print(f"❌ Error: {e}")
    sys.exit(1)
