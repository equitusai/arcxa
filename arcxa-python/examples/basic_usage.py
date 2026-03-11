#!/usr/bin/env python3
"""Basic usage examples for Graphica Python client."""

from graphica import Client


def main():
    # Connect to local server
    client = Client("http://localhost:8080")

    # --- Ontology Operations ---
    print("=== Ontology Operations ===")

    # List ontologies
    result = client.ontology.list()
    print(f"Total ontologies: {result['total']}")

    # Validate ontology syntax
    content = '''
    @prefix ex: <http://example.org/> .
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

    ex:Customer a rdfs:Class ;
        rdfs:label "Customer" .

    ex:hasName a rdf:Property ;
        rdfs:domain ex:Customer ;
        rdfs:range xsd:string .
    '''
    result = client.ontology.validate(content)
    print(f"Validation status: {result['status']}")

    # --- Mapping Operations ---
    print("\n=== Mapping Operations ===")

    # Get mapping statistics
    stats = client.mapping.statistics()
    print(f"Total sessions: {stats['total_sessions']}")
    print(f"Total conflicts: {stats['total_conflicts']}")

    # List sessions
    sessions = client.mapping.list_sessions(limit=5)
    print(f"Sessions found: {sessions['total_count']}")

    # --- Workflow Operations ---
    print("\n=== Workflow Operations ===")

    # List workflows
    workflows = client.workflows.list()
    if isinstance(workflows, list):
        print(f"Workflows found: {len(workflows)}")
    else:
        print(f"Workflows: {workflows}")

    # --- GDPR Operations ---
    print("\n=== GDPR Operations ===")

    # Check for legal holds (example user)
    try:
        holds = client.gdpr.check_legal_holds("user-123")
        print(f"Legal holds: {holds}")
    except Exception as e:
        print(f"GDPR check: {e}")

    print("\nDone!")


if __name__ == "__main__":
    main()
