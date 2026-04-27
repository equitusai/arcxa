import React, { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  Database,
  Loader2,
  Network,
  Plus,
  Save,
  ShieldCheck,
  Trash2,
  Workflow,
} from 'lucide-react';

import type {
  CreateSosSystemRequest,
  SosDataContract,
  SosInterfaceRecord,
  SosSlaMetric,
  SosSystemRecord,
} from '@/api/sosValidation';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import {
  useApproveSosContract,
  useCreateSosContract,
  useCreateSosInterface,
  useCreateSosSystem,
  useDeleteSosContract,
  useDeleteSosInterface,
  useDeleteSosSystem,
  useSignSosContract,
  useSosContracts,
  useSosInterfaces,
  useSosSystems,
  useUpdateSosContract,
  useUpdateSosInterface,
  useUpdateSosSystem,
} from '@/hooks/useSosValidation';

const INTERFACE_DIRECTIONS = ['Provider', 'Consumer', 'Bidirectional'] as const;
const PROTOCOL_OPTIONS = [
  'REST',
  'gRPC',
  'MQTT',
  'AMQP',
  'Kafka',
  'WebSocket',
  'HTTP',
  'HTTPS',
  'TCP',
  'UDP',
] as const;
const DATA_FORMAT_OPTIONS = [
  'JSON',
  'XML',
  'Protobuf',
  'Avro',
  'MessagePack',
  'Parquet',
  'CSV',
  'YAML',
] as const;

export type SosCatalogTab = 'systems' | 'interfaces' | 'contracts';

export interface SosCatalogSelectionState {
  tab: SosCatalogTab;
  systemId?: string | null;
  interfaceId?: string | null;
  contractId?: string | null;
}

interface SosCatalogPanelProps {
  seedTab?: SosCatalogTab;
  seedSystemId?: string | null;
  seedInterfaceId?: string | null;
  seedContractId?: string | null;
  seedToken?: number;
  onSelectionChange?: (state: SosCatalogSelectionState) => void;
}

interface SystemFormState {
  systemId: string;
  systemName: string;
  systemType: string;
  vendor: string;
  version: string;
  classification: string;
  description: string;
  tagsText: string;
  deploymentText: string;
  capabilitiesText: string;
  active: boolean;
}

interface InterfaceFormState {
  systemId: string;
  interfaceId: string;
  interfaceName: string;
  direction: string;
  protocol: string;
  dataFormat: string;
  coordinateSystem: string;
  unitSystem: string;
  schemaText: string;
  metadataText: string;
}

interface SlaMetricFormState {
  name: string;
  operator: string;
  value: string;
  unit: string;
}

interface ContractFormState {
  contractId: string;
  contractName: string;
  providerInterfaceId: string;
  consumerInterfaceId: string;
  description: string;
  tagsText: string;
  transformationRulesText: string;
  slaMetrics: SlaMetricFormState[];
}

export function SosCatalogPanel({
  seedTab,
  seedSystemId,
  seedInterfaceId,
  seedContractId,
  seedToken,
  onSelectionChange,
}: SosCatalogPanelProps) {
  const [activeTab, setActiveTab] = useState<SosCatalogTab>(seedTab ?? 'systems');
  const [selectedSystemId, setSelectedSystemId] = useState<string | null>(seedSystemId ?? null);
  const [selectedInterfaceId, setSelectedInterfaceId] = useState<string | null>(
    seedInterfaceId ?? null
  );
  const [selectedContractId, setSelectedContractId] = useState<string | null>(
    seedContractId ?? null
  );
  const {
    data: systemsResponse,
    isLoading: isLoadingSystems,
    error: systemsError,
  } = useSosSystems();
  const {
    data: interfacesData,
    isLoading: isLoadingInterfaces,
    error: interfacesError,
  } = useSosInterfaces();
  const {
    data: contractsData,
    isLoading: isLoadingContracts,
    error: contractsError,
  } = useSosContracts();

  const systems = systemsResponse?.systems ?? [];
  const interfaces = interfacesData ?? [];
  const contracts = contractsData ?? [];

  useEffect(() => {
    if (seedTab) {
      setActiveTab(seedTab);
    }
  }, [seedTab, seedToken]);

  useEffect(() => {
    if (seedSystemId !== undefined) {
      setSelectedSystemId(seedSystemId);
    }
  }, [seedSystemId, seedToken]);

  useEffect(() => {
    if (seedInterfaceId !== undefined) {
      setSelectedInterfaceId(seedInterfaceId);
    }
  }, [seedInterfaceId, seedToken]);

  useEffect(() => {
    if (seedContractId !== undefined) {
      setSelectedContractId(seedContractId);
    }
  }, [seedContractId, seedToken]);

  useEffect(() => {
    onSelectionChange?.({
      tab: activeTab,
      systemId: selectedSystemId,
      interfaceId: selectedInterfaceId,
      contractId: selectedContractId,
    });
  }, [
    activeTab,
    onSelectionChange,
    selectedContractId,
    selectedInterfaceId,
    selectedSystemId,
  ]);

  return (
    <div className="space-y-4">
      {(systemsError || interfacesError || contractsError) && (
        <Alert variant="destructive">
          <AlertTriangle className="h-4 w-4" />
          <AlertTitle>SoS catalog could not be loaded cleanly</AlertTitle>
          <AlertDescription>
            {systemsError && <p>Systems: {getErrorMessage(systemsError)}</p>}
            {interfacesError && <p>Interfaces: {getErrorMessage(interfacesError)}</p>}
            {contractsError && <p>Contracts: {getErrorMessage(contractsError)}</p>}
          </AlertDescription>
        </Alert>
      )}

      <Tabs
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as SosCatalogTab)}
        className="space-y-4"
      >
        <TabsList>
          <TabsTrigger value="systems">Systems</TabsTrigger>
          <TabsTrigger value="interfaces">Interfaces</TabsTrigger>
          <TabsTrigger value="contracts">Contracts</TabsTrigger>
        </TabsList>

        <TabsContent value="systems">
          <SystemsCatalogTab
            systems={systems}
            interfaces={interfaces}
            isLoading={isLoadingSystems}
            selectedSystemId={selectedSystemId}
            onSelectedSystemIdChange={setSelectedSystemId}
          />
        </TabsContent>

        <TabsContent value="interfaces">
          <InterfacesCatalogTab
            systems={systems}
            interfaces={interfaces}
            isLoading={isLoadingInterfaces}
            selectedInterfaceId={selectedInterfaceId}
            onSelectedInterfaceIdChange={setSelectedInterfaceId}
          />
        </TabsContent>

        <TabsContent value="contracts">
          <ContractsCatalogTab
            interfaces={interfaces}
            contracts={contracts}
            isLoading={isLoadingContracts}
            selectedContractId={selectedContractId}
            onSelectedContractIdChange={setSelectedContractId}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}

function SystemsCatalogTab({
  systems,
  interfaces,
  isLoading,
  selectedSystemId,
  onSelectedSystemIdChange,
}: {
  systems: SosSystemRecord[];
  interfaces: SosInterfaceRecord[];
  isLoading: boolean;
  selectedSystemId: string | null;
  onSelectedSystemIdChange: (systemId: string | null) => void;
}) {
  const [formError, setFormError] = useState<string | null>(null);
  const [formState, setFormState] = useState<SystemFormState>(emptySystemFormState());

  const createSystem = useCreateSosSystem();
  const updateSystem = useUpdateSosSystem();
  const deleteSystem = useDeleteSosSystem();

  const sortedSystems = useMemo(
    () =>
      [...systems].sort(
        (left, right) =>
          left.system_name.localeCompare(right.system_name) ||
          left.system_id.localeCompare(right.system_id)
      ),
    [systems]
  );

  const selectedSystem = useMemo(
    () => systems.find((system) => system.system_id === selectedSystemId) ?? null,
    [selectedSystemId, systems]
  );

  useEffect(() => {
    if (!selectedSystem) {
      setFormState(emptySystemFormState());
      return;
    }

    setFormState(systemRecordToFormState(selectedSystem));
  }, [selectedSystem]);

  useEffect(() => {
    setFormError(null);
  }, [selectedSystemId]);

  const pending = createSystem.isPending || updateSystem.isPending || deleteSystem.isPending;

  const handleSave = async () => {
    setFormError(null);

    if (!formState.systemId.trim() || !formState.systemName.trim() || !formState.systemType.trim()) {
      setFormError('System id, name, and type are required.');
      return;
    }

    const deployment = parseObjectJson(formState.deploymentText, 'deployment');
    const capabilities = parseObjectJson(formState.capabilitiesText, 'capabilities');

    if (!deployment.ok) {
      setFormError(deployment.error);
      return;
    }

    if (!capabilities.ok) {
      setFormError(capabilities.error);
      return;
    }

    try {
      if (selectedSystem) {
        const updated = await updateSystem.mutateAsync({
          id: selectedSystem.system_id,
          request: {
            system_name: formState.systemName.trim(),
            version: formState.version.trim(),
            classification: formState.classification.trim(),
            description: formState.description,
            deployment: deployment.value,
            capabilities: capabilities.value,
            tags: parseTags(formState.tagsText),
            active: formState.active,
          },
        });
        onSelectedSystemIdChange(updated.system_id);
      } else {
        const created = await createSystem.mutateAsync({
          system_id: formState.systemId.trim(),
          system_name: formState.systemName.trim(),
          system_type: formState.systemType.trim(),
          vendor: formState.vendor.trim(),
          version: formState.version.trim(),
          classification: formState.classification.trim(),
          description: emptyToNull(formState.description),
          deployment: deployment.value,
          capabilities: capabilities.value,
          tags: parseTags(formState.tagsText),
        } satisfies CreateSosSystemRequest);
        onSelectedSystemIdChange(created.system_id);
      }
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const handleDelete = async () => {
    if (!selectedSystem) {
      return;
    }

    const confirmed = window.confirm(
      `Delete system '${selectedSystem.system_name}'? This will fail if interfaces still exist.`
    );

    if (!confirmed) {
      return;
    }

    setFormError(null);

    try {
      await deleteSystem.mutateAsync(selectedSystem.system_id);
      onSelectedSystemIdChange(null);
      setFormState(emptySystemFormState());
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  return (
    <div className="grid gap-4 xl:grid-cols-[420px,minmax(0,1fr)]">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Database className="h-4 w-4" />
            Systems Catalog
          </CardTitle>
          <CardDescription>
            Keep system records separate from validation runs so interface and contract ownership stay explicit.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex justify-between gap-2">
            <Badge variant="outline">{systems.length} systems</Badge>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                onSelectedSystemIdChange(null);
                setFormState(emptySystemFormState());
                setFormError(null);
              }}
              disabled={pending}
            >
              <Plus className="mr-2 h-4 w-4" />
              New System
            </Button>
          </div>

          {isLoading ? (
            <LoadingState label="Loading systems catalog..." />
          ) : sortedSystems.length === 0 ? (
            <EmptyState
              title="No systems yet"
              description="Create a system first, then hang interfaces and contracts off that canonical catalog entry."
            />
          ) : (
            <div className="max-h-[640px] space-y-3 overflow-auto pr-1">
              {sortedSystems.map((system) => {
                const interfaceCount = interfaces.filter(
                  (record) => record.system_id === system.system_id
                ).length;
                const isSelected = system.system_id === selectedSystemId;

                return (
                  <button
                    key={system.system_id}
                    type="button"
                    onClick={() => {
                      onSelectedSystemIdChange(system.system_id);
                      setFormError(null);
                    }}
                    className={`w-full rounded-sm border p-3 text-left transition-colors ${
                      isSelected
                        ? 'border-primary bg-primary/5'
                        : 'border-border bg-background hover:bg-background-secondary'
                    }`}
                  >
                    <div className="flex flex-wrap items-start justify-between gap-2">
                      <div className="space-y-1">
                        <div className="font-medium text-foreground">{system.system_name}</div>
                        <div className="text-xs text-muted-foreground">{system.system_id}</div>
                      </div>
                      <Badge variant={system.active ? 'success' : 'outline'}>
                        {system.active ? 'Active' : 'Inactive'}
                      </Badge>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-2">
                      <Badge variant="outline">{system.system_type}</Badge>
                      <Badge variant="outline">{system.vendor || 'Unknown vendor'}</Badge>
                      <Badge variant="outline">{interfaceCount} interfaces</Badge>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{selectedSystem ? 'Edit System' : 'Create System'}</CardTitle>
          <CardDescription>
            Persist the canonical system record here before registering interfaces or pairing systems through contracts.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {formError && <InlineError message={formError} />}

          <div className="grid gap-4 md:grid-cols-2">
            <Field label="System Id">
              <Input
                value={formState.systemId}
                onChange={(event) => updateForm(setFormState, 'systemId', event.target.value)}
                placeholder="system-id"
                disabled={Boolean(selectedSystem) || pending}
              />
            </Field>
            <Field label="System Name">
              <Input
                value={formState.systemName}
                onChange={(event) => updateForm(setFormState, 'systemName', event.target.value)}
                placeholder="Mission Data Broker"
                disabled={pending}
              />
            </Field>
            <Field label="System Type">
              <Input
                value={formState.systemType}
                onChange={(event) => updateForm(setFormState, 'systemType', event.target.value)}
                placeholder="mission.broker"
                disabled={pending}
              />
            </Field>
            <Field label="Vendor">
              <Input
                value={formState.vendor}
                onChange={(event) => updateForm(setFormState, 'vendor', event.target.value)}
                placeholder="Graphica"
                disabled={pending}
              />
            </Field>
            <Field label="Version">
              <Input
                value={formState.version}
                onChange={(event) => updateForm(setFormState, 'version', event.target.value)}
                placeholder="1.0.0"
                disabled={pending}
              />
            </Field>
            <Field label="Classification">
              <Input
                value={formState.classification}
                onChange={(event) => updateForm(setFormState, 'classification', event.target.value)}
                placeholder="UNCLASSIFIED"
                disabled={pending}
              />
            </Field>
          </div>

          <Field label="Description">
            <Textarea
              value={formState.description}
              onChange={(event) => updateForm(setFormState, 'description', event.target.value)}
              placeholder="What the system owns, exposes, or consumes in the SoS."
              disabled={pending}
            />
          </Field>

          <Field label="Tags">
            <Input
              value={formState.tagsText}
              onChange={(event) => updateForm(setFormState, 'tagsText', event.target.value)}
              placeholder="mission, broker, telemetry"
              disabled={pending}
            />
          </Field>

          <div className="grid gap-4 md:grid-cols-2">
            <Field label="Deployment JSON">
              <Textarea
                value={formState.deploymentText}
                onChange={(event) => updateForm(setFormState, 'deploymentText', event.target.value)}
                className="min-h-[180px] font-mono text-xs"
                disabled={pending}
              />
            </Field>
            <Field label="Capabilities JSON">
              <Textarea
                value={formState.capabilitiesText}
                onChange={(event) => updateForm(setFormState, 'capabilitiesText', event.target.value)}
                className="min-h-[180px] font-mono text-xs"
                disabled={pending}
              />
            </Field>
          </div>

          <div className="flex items-center gap-3 rounded-sm border border-border bg-background-secondary p-3">
            <input
              id="system-active"
              type="checkbox"
              checked={formState.active}
              onChange={(event) => updateForm(setFormState, 'active', event.target.checked)}
              disabled={pending}
            />
            <Label htmlFor="system-active">System is active</Label>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button onClick={handleSave} disabled={pending}>
              {pending ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Save className="mr-2 h-4 w-4" />
              )}
              {selectedSystem ? 'Save Changes' : 'Create System'}
            </Button>
            {selectedSystem && (
              <Button variant="destructive" onClick={handleDelete} disabled={pending}>
                <Trash2 className="mr-2 h-4 w-4" />
                Delete System
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function InterfacesCatalogTab({
  systems,
  interfaces,
  isLoading,
  selectedInterfaceId,
  onSelectedInterfaceIdChange,
}: {
  systems: SosSystemRecord[];
  interfaces: SosInterfaceRecord[];
  isLoading: boolean;
  selectedInterfaceId: string | null;
  onSelectedInterfaceIdChange: (interfaceId: string | null) => void;
}) {
  const [formError, setFormError] = useState<string | null>(null);
  const [formState, setFormState] = useState<InterfaceFormState>(() =>
    emptyInterfaceFormState(systems)
  );

  const createInterface = useCreateSosInterface();
  const updateInterface = useUpdateSosInterface();
  const deleteInterface = useDeleteSosInterface();

  const selectedInterface = useMemo(
    () => interfaces.find((record) => record.interface_id === selectedInterfaceId) ?? null,
    [interfaces, selectedInterfaceId]
  );

  const sortedInterfaces = useMemo(
    () =>
      [...interfaces].sort(
        (left, right) =>
          left.interface_name.localeCompare(right.interface_name) ||
          left.interface_id.localeCompare(right.interface_id)
      ),
    [interfaces]
  );

  useEffect(() => {
    if (!selectedInterface) {
      setFormState(emptyInterfaceFormState(systems));
      return;
    }

    setFormState(interfaceRecordToFormState(selectedInterface));
  }, [selectedInterface, systems]);

  useEffect(() => {
    setFormError(null);
  }, [selectedInterfaceId]);

  const pending = createInterface.isPending || updateInterface.isPending || deleteInterface.isPending;

  const handleSave = async () => {
    setFormError(null);

    if (!formState.systemId || !formState.interfaceId.trim() || !formState.interfaceName.trim()) {
      setFormError('System, interface id, and interface name are required.');
      return;
    }

    const schema = parseJsonValue(formState.schemaText, 'schema');
    const metadata = parseObjectJson(formState.metadataText, 'metadata');

    if (!schema.ok) {
      setFormError(schema.error);
      return;
    }

    if (!metadata.ok) {
      setFormError(metadata.error);
      return;
    }

    try {
      if (selectedInterface) {
        const updated = await updateInterface.mutateAsync({
          id: selectedInterface.interface_id,
          request: {
            interface_name: formState.interfaceName.trim(),
            direction: formState.direction,
            schema: schema.value,
            coordinate_system: formState.coordinateSystem,
            unit_system: formState.unitSystem,
            metadata: metadata.value,
          },
        });
        onSelectedInterfaceIdChange(updated.interface_id);
      } else {
        const created = await createInterface.mutateAsync({
          system_id: formState.systemId,
          interface: {
            interface_id: formState.interfaceId.trim(),
            interface_name: formState.interfaceName.trim(),
            direction: formState.direction,
            protocol: formState.protocol,
            data_format: formState.dataFormat,
            schema: schema.value,
            coordinate_system: emptyToUndefined(formState.coordinateSystem),
            unit_system: emptyToUndefined(formState.unitSystem),
            metadata: metadata.value,
          },
        });
        onSelectedInterfaceIdChange(created.interface_id);
      }
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const handleDelete = async () => {
    if (!selectedInterface) {
      return;
    }

    const confirmed = window.confirm(
      `Delete interface '${selectedInterface.interface_name}'? This will fail if contracts still reference it.`
    );

    if (!confirmed) {
      return;
    }

    setFormError(null);

    try {
      await deleteInterface.mutateAsync(selectedInterface.interface_id);
      onSelectedInterfaceIdChange(null);
      setFormState(emptyInterfaceFormState(systems));
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  return (
    <div className="grid gap-4 xl:grid-cols-[420px,minmax(0,1fr)]">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Network className="h-4 w-4" />
            Interfaces Catalog
          </CardTitle>
          <CardDescription>
            Register canonical provider and consumer interfaces using the backend's accepted direction values.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex justify-between gap-2">
            <Badge variant="outline">{interfaces.length} interfaces</Badge>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                onSelectedInterfaceIdChange(null);
                setFormState(emptyInterfaceFormState(systems));
                setFormError(null);
              }}
              disabled={pending}
            >
              <Plus className="mr-2 h-4 w-4" />
              New Interface
            </Button>
          </div>

          {isLoading ? (
            <LoadingState label="Loading interfaces catalog..." />
          ) : sortedInterfaces.length === 0 ? (
            <EmptyState
              title="No interfaces yet"
              description="Create an interface after the owning system exists in the SoS catalog."
            />
          ) : (
            <div className="max-h-[640px] space-y-3 overflow-auto pr-1">
              {sortedInterfaces.map((record) => {
                const isSelected = record.interface_id === selectedInterfaceId;
                return (
                  <button
                    key={record.interface_id}
                    type="button"
                    onClick={() => {
                      onSelectedInterfaceIdChange(record.interface_id);
                      setFormError(null);
                    }}
                    className={`w-full rounded-sm border p-3 text-left transition-colors ${
                      isSelected
                        ? 'border-primary bg-primary/5'
                        : 'border-border bg-background hover:bg-background-secondary'
                    }`}
                  >
                    <div className="space-y-1">
                      <div className="font-medium text-foreground">{record.interface_name}</div>
                      <div className="text-xs text-muted-foreground">{record.interface_id}</div>
                      <div className="text-xs text-muted-foreground">System: {record.system_id}</div>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-2">
                      <Badge variant={directionVariant(record.direction)}>{record.direction}</Badge>
                      <Badge variant="outline">{record.protocol}</Badge>
                      <Badge variant="outline">{record.data_format}</Badge>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{selectedInterface ? 'Edit Interface' : 'Create Interface'}</CardTitle>
          <CardDescription>
            Use exact backend-supported direction, protocol, and data format values to keep SoS validation canonical.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {formError && <InlineError message={formError} />}

          <Alert>
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Direction values matter</AlertTitle>
            <AlertDescription>
              The coordinator currently validates interface directions as
              {' '}
              <span className="font-mono">Provider</span>,
              {' '}
              <span className="font-mono">Consumer</span>, or
              {' '}
              <span className="font-mono">Bidirectional</span>.
            </AlertDescription>
          </Alert>

          <div className="grid gap-4 md:grid-cols-2">
            <Field label="Owning System">
              <Select
                value={formState.systemId || undefined}
                onValueChange={(value) => updateForm(setFormState, 'systemId', value)}
                disabled={Boolean(selectedInterface) || pending || systems.length === 0}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Choose a system" />
                </SelectTrigger>
                <SelectContent>
                  {systems.map((system) => (
                    <SelectItem key={system.system_id} value={system.system_id}>
                      {system.system_name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="Interface Id">
              <Input
                value={formState.interfaceId}
                onChange={(event) => updateForm(setFormState, 'interfaceId', event.target.value)}
                placeholder="iface.mission.out"
                disabled={Boolean(selectedInterface) || pending}
              />
            </Field>
            <Field label="Interface Name">
              <Input
                value={formState.interfaceName}
                onChange={(event) => updateForm(setFormState, 'interfaceName', event.target.value)}
                placeholder="Mission Output"
                disabled={pending}
              />
            </Field>
            <Field label="Direction">
              <Select
                value={formState.direction}
                onValueChange={(value) => updateForm(setFormState, 'direction', value)}
                disabled={pending}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Choose direction" />
                </SelectTrigger>
                <SelectContent>
                  {INTERFACE_DIRECTIONS.map((value) => (
                    <SelectItem key={value} value={value}>
                      {value}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="Protocol">
              <Select
                value={formState.protocol}
                onValueChange={(value) => updateForm(setFormState, 'protocol', value)}
                disabled={Boolean(selectedInterface) || pending}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Choose protocol" />
                </SelectTrigger>
                <SelectContent>
                  {PROTOCOL_OPTIONS.map((value) => (
                    <SelectItem key={value} value={value}>
                      {value}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="Data Format">
              <Select
                value={formState.dataFormat}
                onValueChange={(value) => updateForm(setFormState, 'dataFormat', value)}
                disabled={Boolean(selectedInterface) || pending}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Choose data format" />
                </SelectTrigger>
                <SelectContent>
                  {DATA_FORMAT_OPTIONS.map((value) => (
                    <SelectItem key={value} value={value}>
                      {value}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="Coordinate System">
              <Input
                value={formState.coordinateSystem}
                onChange={(event) => updateForm(setFormState, 'coordinateSystem', event.target.value)}
                placeholder="WGS84"
                disabled={pending}
              />
            </Field>
            <Field label="Unit System">
              <Input
                value={formState.unitSystem}
                onChange={(event) => updateForm(setFormState, 'unitSystem', event.target.value)}
                placeholder="SI"
                disabled={pending}
              />
            </Field>
          </div>

          <Field label="Schema JSON">
            <Textarea
              value={formState.schemaText}
              onChange={(event) => updateForm(setFormState, 'schemaText', event.target.value)}
              className="min-h-[220px] font-mono text-xs"
              disabled={pending}
            />
          </Field>

          <Field label="Metadata JSON">
            <Textarea
              value={formState.metadataText}
              onChange={(event) => updateForm(setFormState, 'metadataText', event.target.value)}
              className="min-h-[180px] font-mono text-xs"
              disabled={pending}
            />
          </Field>

          <div className="flex flex-wrap gap-2">
            <Button onClick={handleSave} disabled={pending || systems.length === 0}>
              {pending ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Save className="mr-2 h-4 w-4" />
              )}
              {selectedInterface ? 'Save Changes' : 'Create Interface'}
            </Button>
            {selectedInterface && (
              <Button variant="destructive" onClick={handleDelete} disabled={pending}>
                <Trash2 className="mr-2 h-4 w-4" />
                Delete Interface
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function ContractsCatalogTab({
  interfaces,
  contracts,
  isLoading,
  selectedContractId,
  onSelectedContractIdChange,
}: {
  interfaces: SosInterfaceRecord[];
  contracts: SosDataContract[];
  isLoading: boolean;
  selectedContractId: string | null;
  onSelectedContractIdChange: (contractId: string | null) => void;
}) {
  const [formError, setFormError] = useState<string | null>(null);
  const [formState, setFormState] = useState<ContractFormState>(() =>
    emptyContractFormState(interfaces)
  );

  const createContract = useCreateSosContract();
  const updateContract = useUpdateSosContract();
  const deleteContract = useDeleteSosContract();
  const approveContract = useApproveSosContract();
  const signContract = useSignSosContract();

  const selectedContract = useMemo(
    () => contracts.find((contract) => contract.contract_id === selectedContractId) ?? null,
    [contracts, selectedContractId]
  );

  const sortedContracts = useMemo(
    () =>
      [...contracts].sort(
        (left, right) => right.updated_at.localeCompare(left.updated_at)
      ),
    [contracts]
  );

  const providerInterfaces = useMemo(
    () => interfaces.filter((record) => record.direction === 'Provider'),
    [interfaces]
  );
  const consumerInterfaces = useMemo(
    () => interfaces.filter((record) => record.direction === 'Consumer'),
    [interfaces]
  );

  useEffect(() => {
    if (!selectedContract) {
      setFormState(emptyContractFormState(interfaces));
      return;
    }

    setFormState(contractRecordToFormState(selectedContract));
  }, [selectedContract, interfaces]);

  useEffect(() => {
    setFormError(null);
  }, [selectedContractId]);

  const pending =
    createContract.isPending ||
    updateContract.isPending ||
    deleteContract.isPending ||
    approveContract.isPending ||
    signContract.isPending;

  const contractLocked = Boolean(selectedContract?.signed);

  const handleSave = async () => {
    setFormError(null);

    if (
      !formState.contractId.trim() ||
      !formState.contractName.trim() ||
      !formState.providerInterfaceId ||
      !formState.consumerInterfaceId
    ) {
      setFormError('Contract id, name, provider interface, and consumer interface are required.');
      return;
    }

    const transformationRules = parseObjectJson(
      formState.transformationRulesText,
      'transformation rules'
    );
    if (!transformationRules.ok) {
      setFormError(transformationRules.error);
      return;
    }

    const slaMetrics = parseSlaMetrics(formState.slaMetrics);
    if (!slaMetrics.ok) {
      setFormError(slaMetrics.error);
      return;
    }

    try {
      if (selectedContract) {
        const updated = await updateContract.mutateAsync({
          id: selectedContract.contract_id,
          request: {
            contract_name: formState.contractName.trim(),
            sla_metrics: slaMetrics.value,
            transformation_rules: transformationRules.value,
            description: formState.description,
            tags: parseTags(formState.tagsText),
          },
        });
        onSelectedContractIdChange(updated.contract_id);
      } else {
        const created = await createContract.mutateAsync({
          contract_id: formState.contractId.trim(),
          contract_name: formState.contractName.trim(),
          provider_interface_id: formState.providerInterfaceId,
          consumer_interface_id: formState.consumerInterfaceId,
          sla_metrics: slaMetrics.value,
          transformation_rules: transformationRules.value,
          description: emptyToNull(formState.description),
          tags: parseTags(formState.tagsText),
        });
        onSelectedContractIdChange(created.contract_id);
      }
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const handleDelete = async () => {
    if (!selectedContract) {
      return;
    }

    const confirmed = window.confirm(
      `Delete contract '${selectedContract.contract_name}'? Signed contracts cannot be deleted.`
    );

    if (!confirmed) {
      return;
    }

    setFormError(null);

    try {
      await deleteContract.mutateAsync(selectedContract.contract_id);
      onSelectedContractIdChange(null);
      setFormState(emptyContractFormState(interfaces));
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const handleApprove = async () => {
    if (!selectedContract) {
      return;
    }

    setFormError(null);

    try {
      const approved = await approveContract.mutateAsync(selectedContract.contract_id);
      onSelectedContractIdChange(approved.contract_id);
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  const handleSign = async () => {
    if (!selectedContract) {
      return;
    }

    setFormError(null);

    try {
      const signed = await signContract.mutateAsync(selectedContract.contract_id);
      onSelectedContractIdChange(signed.contract_id);
    } catch (error) {
      setFormError(getErrorMessage(error));
    }
  };

  return (
    <div className="grid gap-4 xl:grid-cols-[420px,minmax(0,1fr)]">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Workflow className="h-4 w-4" />
            Contracts Catalog
          </CardTitle>
          <CardDescription>
            Manage interface-pair contracts and their governance state from the same SoS area.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex justify-between gap-2">
            <Badge variant="outline">{contracts.length} contracts</Badge>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                onSelectedContractIdChange(null);
                setFormState(emptyContractFormState(interfaces));
                setFormError(null);
              }}
              disabled={pending}
            >
              <Plus className="mr-2 h-4 w-4" />
              New Contract
            </Button>
          </div>

          {isLoading ? (
            <LoadingState label="Loading contracts catalog..." />
          ) : sortedContracts.length === 0 ? (
            <EmptyState
              title="No contracts yet"
              description="Create contracts once Provider and Consumer interfaces exist."
            />
          ) : (
            <div className="max-h-[640px] space-y-3 overflow-auto pr-1">
              {sortedContracts.map((contract) => {
                const isSelected = contract.contract_id === selectedContractId;
                return (
                  <button
                    key={contract.contract_id}
                    type="button"
                    onClick={() => {
                      onSelectedContractIdChange(contract.contract_id);
                      setFormError(null);
                    }}
                    className={`w-full rounded-sm border p-3 text-left transition-colors ${
                      isSelected
                        ? 'border-primary bg-primary/5'
                        : 'border-border bg-background hover:bg-background-secondary'
                    }`}
                  >
                    <div className="flex flex-wrap items-start justify-between gap-2">
                      <div className="space-y-1">
                        <div className="font-medium text-foreground">{contract.contract_name}</div>
                        <div className="text-xs text-muted-foreground">{contract.contract_id}</div>
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <Badge variant={contract.approved ? 'success' : 'warning'}>
                          {contract.approved ? 'Approved' : 'Pending Approval'}
                        </Badge>
                        <Badge variant={contract.signed ? 'success' : 'outline'}>
                          {contract.signed ? 'Signed' : 'Unsigned'}
                        </Badge>
                      </div>
                    </div>
                    <div className="mt-3 space-y-1 text-xs text-muted-foreground">
                      <div>Provider: {contract.provider_interface_id}</div>
                      <div>Consumer: {contract.consumer_interface_id}</div>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{selectedContract ? 'Edit Contract' : 'Create Contract'}</CardTitle>
          <CardDescription>
            Contract creation currently requires the provider interface direction to be exactly
            {' '}
            <span className="font-mono">Provider</span>
            {' '}
            and the consumer direction to be exactly
            {' '}
            <span className="font-mono">Consumer</span>.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {formError && <InlineError message={formError} />}

          {(providerInterfaces.length === 0 || consumerInterfaces.length === 0) && (
            <Alert>
              <AlertTriangle className="h-4 w-4" />
              <AlertTitle>Contract creation is blocked</AlertTitle>
              <AlertDescription>
                You need at least one
                {' '}
                <span className="font-mono">Provider</span>
                {' '}
                interface and one
                {' '}
                <span className="font-mono">Consumer</span>
                {' '}
                interface before the backend will accept a contract pair.
              </AlertDescription>
            </Alert>
          )}

          {contractLocked && (
            <Alert>
              <ShieldCheck className="h-4 w-4" />
              <AlertTitle>Signed contract</AlertTitle>
              <AlertDescription>
                Signed contracts are immutable. Governance actions remain visible here, but editing is disabled.
              </AlertDescription>
            </Alert>
          )}

          <div className="grid gap-4 md:grid-cols-2">
            <Field label="Contract Id">
              <Input
                value={formState.contractId}
                onChange={(event) => updateForm(setFormState, 'contractId', event.target.value)}
                placeholder="contract.provider.to.consumer"
                disabled={Boolean(selectedContract) || pending}
              />
            </Field>
            <Field label="Contract Name">
              <Input
                value={formState.contractName}
                onChange={(event) => updateForm(setFormState, 'contractName', event.target.value)}
                placeholder="Provider To Consumer Contract"
                disabled={pending || contractLocked}
              />
            </Field>
            <Field label="Provider Interface">
              <Select
                value={formState.providerInterfaceId || undefined}
                onValueChange={(value) => updateForm(setFormState, 'providerInterfaceId', value)}
                disabled={Boolean(selectedContract) || pending}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Choose provider interface" />
                </SelectTrigger>
                <SelectContent>
                  {providerInterfaces.map((record) => (
                    <SelectItem key={record.interface_id} value={record.interface_id}>
                      {record.interface_name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="Consumer Interface">
              <Select
                value={formState.consumerInterfaceId || undefined}
                onValueChange={(value) => updateForm(setFormState, 'consumerInterfaceId', value)}
                disabled={Boolean(selectedContract) || pending}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Choose consumer interface" />
                </SelectTrigger>
                <SelectContent>
                  {consumerInterfaces.map((record) => (
                    <SelectItem key={record.interface_id} value={record.interface_id}>
                      {record.interface_name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          </div>

          <Field label="Description">
            <Textarea
              value={formState.description}
              onChange={(event) => updateForm(setFormState, 'description', event.target.value)}
              placeholder="What this interface pair guarantees operationally."
              disabled={pending || contractLocked}
            />
          </Field>

          <Field label="Tags">
            <Input
              value={formState.tagsText}
              onChange={(event) => updateForm(setFormState, 'tagsText', event.target.value)}
              placeholder="mission, signed, low-latency"
              disabled={pending || contractLocked}
            />
          </Field>

          <Field label="Transformation Rules JSON">
            <Textarea
              value={formState.transformationRulesText}
              onChange={(event) => updateForm(setFormState, 'transformationRulesText', event.target.value)}
              className="min-h-[180px] font-mono text-xs"
              disabled={pending || contractLocked}
            />
          </Field>

          <div className="space-y-3 rounded-sm border border-border bg-background-secondary p-4">
            <div className="flex items-center justify-between gap-2">
              <div>
                <div className="text-sm font-semibold text-foreground">SLA Metrics</div>
                <div className="text-xs text-muted-foreground">
                  Add explicit SLA expectations the coordinator can validate.
                </div>
              </div>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  updateForm(setFormState, 'slaMetrics', [
                    ...formState.slaMetrics,
                    emptySlaMetricFormState(),
                  ])
                }
                disabled={pending || contractLocked}
              >
                <Plus className="mr-2 h-4 w-4" />
                Add Metric
              </Button>
            </div>

            <div className="space-y-3">
              {formState.slaMetrics.map((metric, index) => (
                <div key={`sla-metric-${index}`} className="grid gap-3 rounded-sm border border-border bg-background p-3 md:grid-cols-[1.4fr,0.8fr,0.8fr,0.8fr,auto]">
                  <Input
                    value={metric.name}
                    onChange={(event) => updateSlaMetric(setFormState, index, 'name', event.target.value)}
                    placeholder="latency_ms"
                    disabled={pending || contractLocked}
                  />
                  <Input
                    value={metric.operator}
                    onChange={(event) => updateSlaMetric(setFormState, index, 'operator', event.target.value)}
                    placeholder="<="
                    disabled={pending || contractLocked}
                  />
                  <Input
                    value={metric.value}
                    onChange={(event) => updateSlaMetric(setFormState, index, 'value', event.target.value)}
                    placeholder="100"
                    disabled={pending || contractLocked}
                  />
                  <Input
                    value={metric.unit}
                    onChange={(event) => updateSlaMetric(setFormState, index, 'unit', event.target.value)}
                    placeholder="ms"
                    disabled={pending || contractLocked}
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => removeSlaMetric(setFormState, index)}
                    disabled={pending || contractLocked || formState.slaMetrics.length === 1}
                    title="Remove metric"
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              ))}
            </div>
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              onClick={handleSave}
              disabled={pending || contractLocked || providerInterfaces.length === 0 || consumerInterfaces.length === 0}
            >
              {pending ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Save className="mr-2 h-4 w-4" />
              )}
              {selectedContract ? 'Save Changes' : 'Create Contract'}
            </Button>
            {selectedContract && !selectedContract.approved && (
              <Button variant="outline" onClick={handleApprove} disabled={pending}>
                Approve Contract
              </Button>
            )}
            {selectedContract && selectedContract.approved && !selectedContract.signed && (
              <Button variant="outline" onClick={handleSign} disabled={pending}>
                Sign Contract
              </Button>
            )}
            {selectedContract && !selectedContract.signed && (
              <Button variant="destructive" onClick={handleDelete} disabled={pending}>
                <Trash2 className="mr-2 h-4 w-4" />
                Delete Contract
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function emptySystemFormState(): SystemFormState {
  return {
    systemId: '',
    systemName: '',
    systemType: '',
    vendor: '',
    version: '1.0.0',
    classification: 'UNCLASSIFIED',
    description: '',
    tagsText: '',
    deploymentText: '{}',
    capabilitiesText: '{}',
    active: true,
  };
}

function emptyInterfaceFormState(systems: SosSystemRecord[]): InterfaceFormState {
  return {
    systemId: systems[0]?.system_id ?? '',
    interfaceId: '',
    interfaceName: '',
    direction: 'Provider',
    protocol: 'REST',
    dataFormat: 'JSON',
    coordinateSystem: '',
    unitSystem: '',
    schemaText: '{}',
    metadataText: '{}',
  };
}

function emptySlaMetricFormState(): SlaMetricFormState {
  return {
    name: '',
    operator: '<=',
    value: '',
    unit: '',
  };
}

function emptyContractFormState(interfaces: SosInterfaceRecord[]): ContractFormState {
  const defaultProvider = interfaces.find((record) => record.direction === 'Provider');
  const defaultConsumer = interfaces.find((record) => record.direction === 'Consumer');

  return {
    contractId: '',
    contractName: '',
    providerInterfaceId: defaultProvider?.interface_id ?? '',
    consumerInterfaceId: defaultConsumer?.interface_id ?? '',
    description: '',
    tagsText: '',
    transformationRulesText: '{}',
    slaMetrics: [emptySlaMetricFormState()],
  };
}

function systemRecordToFormState(system: SosSystemRecord): SystemFormState {
  return {
    systemId: system.system_id,
    systemName: system.system_name,
    systemType: system.system_type,
    vendor: system.vendor,
    version: system.version,
    classification: system.classification,
    description: system.description ?? '',
    tagsText: system.tags.join(', '),
    deploymentText: formatJson(system.deployment),
    capabilitiesText: formatJson(system.capabilities),
    active: system.active,
  };
}

function interfaceRecordToFormState(record: SosInterfaceRecord): InterfaceFormState {
  return {
    systemId: record.system_id,
    interfaceId: record.interface_id,
    interfaceName: record.interface_name,
    direction: record.direction,
    protocol: record.protocol,
    dataFormat: record.data_format,
    coordinateSystem: record.coordinate_system ?? '',
    unitSystem: record.unit_system ?? '',
    schemaText: formatJson(record.schema),
    metadataText: formatJson(record.metadata),
  };
}

function contractRecordToFormState(contract: SosDataContract): ContractFormState {
  return {
    contractId: contract.contract_id,
    contractName: contract.contract_name,
    providerInterfaceId: contract.provider_interface_id,
    consumerInterfaceId: contract.consumer_interface_id,
    description: contract.description ?? '',
    tagsText: contract.tags.join(', '),
    transformationRulesText: formatJson(contract.transformation_rules),
    slaMetrics:
      contract.sla_metrics.length > 0
        ? contract.sla_metrics.map((metric) => ({
            name: metric.name,
            operator: metric.operator,
            value: String(metric.value),
            unit: metric.unit ?? '',
          }))
        : [emptySlaMetricFormState()],
  };
}

function parseTags(value: string): string[] {
  return value
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function parseObjectJson(
  rawText: string,
  label: string
): { ok: true; value: Record<string, unknown> } | { ok: false; error: string } {
  try {
    const parsed = rawText.trim() ? JSON.parse(rawText) : {};
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return { ok: true, value: parsed as Record<string, unknown> };
    }
    return { ok: false, error: `${label} must be a JSON object.` };
  } catch (error) {
    return { ok: false, error: `${label} is not valid JSON: ${getErrorMessage(error)}` };
  }
}

function parseJsonValue(
  rawText: string,
  label: string
): { ok: true; value: unknown } | { ok: false; error: string } {
  try {
    const parsed = rawText.trim() ? JSON.parse(rawText) : {};
    return { ok: true, value: parsed };
  } catch (error) {
    return { ok: false, error: `${label} is not valid JSON: ${getErrorMessage(error)}` };
  }
}

function parseSlaMetrics(
  metrics: SlaMetricFormState[]
): { ok: true; value: SosSlaMetric[] } | { ok: false; error: string } {
  const normalized: SosSlaMetric[] = [];

  for (const metric of metrics) {
    const isBlank = !metric.name.trim() && !metric.operator.trim() && !metric.value.trim() && !metric.unit.trim();
    if (isBlank) {
      continue;
    }

    if (!metric.name.trim() || !metric.operator.trim() || !metric.value.trim()) {
      return {
        ok: false,
        error: 'Each SLA metric needs a name, operator, and numeric value.',
      };
    }

    const parsedValue = Number(metric.value);
    if (Number.isNaN(parsedValue)) {
      return {
        ok: false,
        error: `SLA metric '${metric.name || 'unnamed'}' must have a numeric value.`,
      };
    }

    normalized.push({
      name: metric.name.trim(),
      operator: metric.operator.trim(),
      value: parsedValue,
      unit: emptyToUndefined(metric.unit),
    });
  }

  return { ok: true, value: normalized };
}

function updateSlaMetric(
  setFormState: React.Dispatch<React.SetStateAction<ContractFormState>>,
  index: number,
  field: keyof SlaMetricFormState,
  value: string
) {
  setFormState((current) => ({
    ...current,
    slaMetrics: current.slaMetrics.map((metric, metricIndex) =>
      metricIndex === index ? { ...metric, [field]: value } : metric
    ),
  }));
}

function removeSlaMetric(
  setFormState: React.Dispatch<React.SetStateAction<ContractFormState>>,
  index: number
) {
  setFormState((current) => ({
    ...current,
    slaMetrics:
      current.slaMetrics.length === 1
        ? current.slaMetrics
        : current.slaMetrics.filter((_, metricIndex) => metricIndex !== index),
  }));
}

function updateForm<T, K extends keyof T>(
  setState: React.Dispatch<React.SetStateAction<T>>,
  key: K,
  value: T[K]
) {
  setState((current) => ({
    ...current,
    [key]: value,
  }));
}

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed ? trimmed : null;
}

function emptyToUndefined(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function formatJson(value: unknown): string {
  return JSON.stringify(value ?? {}, null, 2);
}

function directionVariant(direction: string): 'default' | 'secondary' | 'outline' {
  if (direction === 'Provider') {
    return 'default';
  }
  if (direction === 'Consumer') {
    return 'secondary';
  }
  return 'outline';
}

function LoadingState({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 text-sm text-muted-foreground">
      <Loader2 className="h-4 w-4 animate-spin" />
      <span>{label}</span>
    </div>
  );
}

function InlineError({ message }: { message: string }) {
  return (
    <Alert variant="destructive">
      <AlertTriangle className="h-4 w-4" />
      <AlertTitle>Request failed</AlertTitle>
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  );
}

function EmptyState({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div className="rounded-sm border border-dashed border-border p-6 text-center">
      <p className="font-medium text-foreground">{title}</p>
      <p className="mt-2 text-sm text-muted-foreground">{description}</p>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-2">
      <Label>{label}</Label>
      {children}
    </div>
  );
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  const apiError = error as {
    message?: string;
    response?: {
      data?: {
        message?: string;
        error?: string;
      };
    };
  };

  return (
    apiError.response?.data?.message ||
    apiError.response?.data?.error ||
    apiError.message ||
    'Request failed'
  );
}
