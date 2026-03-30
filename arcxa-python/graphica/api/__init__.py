"""API modules for Graphica client."""

from graphica.api.ontology import OntologyAPI
from graphica.api.mapping import MappingAPI
from graphica.api.lineage import LineageAPI
from graphica.api.loader import LoaderAPI
from graphica.api.workflows import WorkflowsAPI
from graphica.api.datasources import DatasourcesAPI
from graphica.api.datasets import DatasetsAPI
from graphica.api.gdpr import GdprAPI
from graphica.api.r2rml import R2rmlAPI

__all__ = [
    "OntologyAPI",
    "MappingAPI",
    "LineageAPI",
    "LoaderAPI",
    "WorkflowsAPI",
    "DatasourcesAPI",
    "DatasetsAPI",
    "GdprAPI",
    "R2rmlAPI",
]
