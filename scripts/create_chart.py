import os
import graphviz

class FinalSemanticArchitecturalMap:
    """
    Generates a final, semantic-focused architectural map of the Graphica application.
    """
    def __init__(self):
        self.dot = graphviz.Digraph(
            'GraphicaArchitecture',
            comment='Graphica Semantic Architectural Map'
        )
        self.setup_styles()

    def setup_styles(self):
        """Defines the visual style for the graph."""
        self.dot.attr(
            'graph',
            rankdir='TB',
            splines='spline',
            nodesep='0.8',
            ranksep='1.5',
            bgcolor='#FFFFFF',
            label='Graphica Architectural Map (Semantic & Governance Layers)',
            fontname='Helvetica, Arial, sans-serif',
            fontsize='32',
            labeljust='l',
            compound='true'
        )
        self.dot.attr('node', style='rounded,filled', shape='box', fontname='Helvetica, Arial, sans-serif')
        self.dot.attr('edge', color='#444444', arrowhead='vee', fontsize='11', fontname='Helvetica, Arial, sans-serif')
        
        self.palette = {
            'clients': '#E1E1E1',
            'app_layer_bg': '#E6F2FF',
            'semantic_layer_bg': '#FFF9E6',
            'data_layer_bg': '#D4EDDA',
            'governance_layer': '#F0E6FF',
            'major_engine': '#FFF0B3',
            'api': '#B3D9FF',
            'primitives': '#D1C4E9',
            'core_feature': '#FFFFFF',
            'governance_edge': '#6A5ACD'
        }

    def generate(self):
        """Builds and defines the architectural components of the graph."""

        # --- Define Layers and Nodes ---
        with self.dot.subgraph(name='cluster_app_layer') as c:
            c.attr(label='Application & Orchestration Layer', style='rounded,filled', fillcolor=self.palette['app_layer_bg'], fontsize='20')
            c.node('api', 'Public API Layer\n(REST: Axum, gRPC: Tonic)', shape='cds', fillcolor=self.palette['api'], fontsize='14')
            c.node('workflows', 'Workflow Engine\n(Conditional Routing, Transactions)', fillcolor=self.palette['major_engine'], fontsize='14')
            c.node('primitives', 'Distributed Primitives\n(Consensus, Health Checks)', fillcolor=self.palette['primitives'], fontsize='12', shape='octagon')
            # Force horizontal alignment
            c.body.append('{rank=same; api; workflows; primitives}')

        with self.dot.subgraph(name='cluster_semantic_layer') as c:
            c.attr(label='Semantic Layer', style='rounded,filled', fillcolor=self.palette['semantic_layer_bg'], fontsize='20')
            c.node('mapping', 'Schema Intelligence Engine\n(R2RML, SHACL, PROV)', fillcolor=self.palette['major_engine'], fontsize='14')
            c.node('bitemporal', 'Bitemporal Engine\n(Immutable History)', fillcolor=self.palette['major_engine'], fontsize='14')
            c.body.append('{rank=same; mapping; bitemporal}')

        with self.dot.subgraph(name='cluster_data_layer') as c:
            c.attr(label='Data & Foundation Layer', style='rounded,filled', fillcolor=self.palette['data_layer_bg'], fontsize='20')
            c.node('rdf_storage', 'RDF Triple Store\n(Oxigraph + RocksDB)', shape='cylinder', fontsize='14')
            c.node('core_types', 'Shared Domain Types\n(LineageEvent, Proto)', fillcolor=self.palette['core_feature'], fontsize='14')
            c.node('model_service', 'ML Model Serving', shape='cds', fontsize='14')
            c.body.append('{rank=same; rdf_storage; core_types; model_service}')

        self.dot.node('clients', 'External Clients\n(Web UI, API Users)', shape='cylinder', fillcolor=self.palette['clients'])
        self.dot.node('governance_layer', 'Unified Governance Layer\n(Lineage & Audit Trail)', shape='tab', fillcolor=self.palette['governance_layer'], fontsize='16')

        # --- Define Edges ---
        self.dot.edge('clients', 'api', label='sends requests')
        self.dot.edge('api', 'workflows', label='triggers')
        self.dot.edge('workflows', 'mapping', label='uses for validation')
        self.dot.edge('mapping', 'rdf_storage', label='Defines & Validates\nSemantic Data')
        self.dot.edge('bitemporal', 'rdf_storage', label='Stores Bitemporal\nData as RDF')
        self.dot.edge('api', 'rdf_storage', label='Federated Queries\n(SPARQL over gRPC)', style='dashed', color='#006400')
        self.dot.edge('bitemporal', 'primitives', label='uses for consensus')
        self.dot.edge('primitives', 'rdf_storage', label='Health Checks', style='dashed', color='#483D8B')
        self.dot.edge('workflows', 'model_service', label='Inference Requests (gRPC)', style='dashed', color='#00008B')
        self.dot.edge('bitemporal', 'core_types', label='uses')
        self.dot.edge('workflows', 'core_types', label='uses')
        self.dot.edge('mapping', 'core_types', label='uses')
        self.dot.edge('rdf_storage', 'core_types', label='uses')
        self.dot.edge('bitemporal', 'governance_layer', label='Records Immutable History', style='dotted', color=self.palette['governance_edge'], arrowhead='odot', constraint='false')
        self.dot.edge('mapping', 'governance_layer', label='Records Schema Evolution (PROV)', style='dotted', color=self.palette['governance_edge'], arrowhead='odot', constraint='false')
        self.dot.edge('workflows', 'governance_layer', label='Records Action & Data Lineage', style='dotted', color=self.palette['governance_edge'], arrowhead='odot', constraint='false')

    def render(self, output_filename):
        """Renders the graph to a file."""
        try:
            full_output_path = os.path.join(os.path.dirname(__file__), output_filename)
            self.dot.render(full_output_path, format='png', view=False, cleanup=True)
            print(f"Successfully generated final semantic map: {full_output_path}.png")
        except Exception as e:
            print(f"Error rendering graph: {e}")

if __name__ == '__main__':
    arch_map = FinalSemanticArchitecturalMap()
    arch_map.generate()
    arch_map.render('graphica_semantic_architectural_map')