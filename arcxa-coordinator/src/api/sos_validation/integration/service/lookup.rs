use super::*;

pub(super) fn get_system(
    service: &SosValidationService,
    system_id: &str,
) -> Result<System, SosValidationServiceError> {
    service
        .storage_manager
        .get_system(system_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("System '{}' not found", system_id))
        })
}

pub(super) fn get_interface(
    service: &SosValidationService,
    interface_id: &str,
) -> Result<Interface, SosValidationServiceError> {
    service
        .storage_manager
        .get_interface(interface_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Interface '{}' not found", interface_id))
        })
}

pub(super) fn get_contract(
    service: &SosValidationService,
    contract_id: &str,
) -> Result<Contract, SosValidationServiceError> {
    service
        .storage_manager
        .get_contract(contract_id)
        .map_err(map_storage_error)?
        .ok_or_else(|| {
            SosValidationServiceError::NotFound(format!("Contract '{}' not found", contract_id))
        })
}

pub(super) fn find_contract_between(
    service: &SosValidationService,
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> Result<Option<Contract>, SosValidationServiceError> {
    service
        .storage_manager
        .get_contract_by_interface_pair(provider_interface_id, consumer_interface_id)
        .map_err(map_storage_error)
}

pub(super) fn find_contract_between_catalog<'a>(
    contracts: &'a HashMap<String, Contract>,
    provider_interface_id: &str,
    consumer_interface_id: &str,
) -> Option<&'a Contract> {
    contracts.values().find(|contract| {
        contract.provider_interface_id == provider_interface_id
            && contract.consumer_interface_id == consumer_interface_id
    })
}
