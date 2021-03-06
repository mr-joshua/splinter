// Copyright 2018-2020 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Defines a YAML backed implementation of the `AdminServiceStore`. The goal of this
//! implementation is to support Splinter v0.4 YAML state files.
//!
//! The public interface includes the struct [`YamlAdminServiceStore`].
//!
//! [`YamlAdminServiceStore`]: struct.YamlAdminServiceStore.html

pub mod error;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use self::error::YamlAdminStoreError;

use super::{
    AdminServiceStore, AdminServiceStoreError, AuthorizationType, Circuit, CircuitNode,
    CircuitPredicate, CircuitProposal, DurabilityType, PersistenceType, RouteType, Service,
    ServiceId,
};

/// A YAML backed implementation of the `AdminServiceStore`
pub struct YamlAdminServiceStore {
    circuit_file_path: String,
    proposal_file_path: String,
    state: Arc<Mutex<YamlState>>,
}

impl YamlAdminServiceStore {
    /// Creates a new `YamlAdminServiceStore`. If the file paths provided exist, the existing state
    /// will be cached in the store. If the files do not exist, they will be created with empty
    /// state.
    ///
    /// # Arguments
    ///
    ///  * `circuit_file_path` - The path to file that contains circuit state
    ///  * `proposal_file_path` - The path to file that contains circuit proposal state
    ///
    /// Returns an error if the file paths cannot be read from or written to
    pub fn new(
        circuit_file_path: String,
        proposal_file_path: String,
    ) -> Result<Self, YamlAdminStoreError> {
        let mut store = YamlAdminServiceStore {
            circuit_file_path: circuit_file_path.to_string(),
            proposal_file_path: proposal_file_path.to_string(),
            state: Arc::new(Mutex::new(YamlState::default())),
        };

        let circuit_file_path_buf = PathBuf::from(circuit_file_path);
        let proposal_file_path_buf = PathBuf::from(proposal_file_path);

        // If file already exists, read it; otherwise initialize it.
        if circuit_file_path_buf.is_file() && proposal_file_path_buf.is_file() {
            store.read_state()?;
        } else if circuit_file_path_buf.is_file() {
            // read circuit
            store.read_circuit_state()?;
            // write proposals
            store.write_proposal_state()?;
        } else if proposal_file_path_buf.is_file() {
            // write circuit
            store.write_circuit_state()?;
            // read proposals
            store.read_proposal_state()?;
        } else {
            // write all empty state
            store.write_state()?;
        }

        Ok(store)
    }

    /// Read circuit state from the circuit file path and cache the contents in the store
    fn read_circuit_state(&mut self) -> Result<(), YamlAdminStoreError> {
        let circuit_file = File::open(&self.circuit_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to open YAML circuit state file",
                Box::new(err),
            )
        })?;

        let yaml_state_circuits: YamlCircuitState = serde_yaml::from_reader(&circuit_file)
            .map_err(|err| {
                YamlAdminStoreError::general_error_with_source(
                    "Failed to read YAML circuit state file",
                    Box::new(err),
                )
            })?;

        let yaml_state = CircuitState::from(yaml_state_circuits);

        let mut state = self.state.lock().map_err(|_| {
            YamlAdminStoreError::general_error("YAML admin service store's internal lock poisoned")
        })?;

        for (circuit_id, circuit) in yaml_state.circuits.iter() {
            for service in circuit.roster.iter() {
                let service_id =
                    ServiceId::new(service.service_id.to_string(), circuit_id.to_string());

                state.service_directory.insert(service_id, service.clone());
            }
        }

        state.circuit_state = yaml_state;
        Ok(())
    }

    /// Read circuit proposal state from the proposal file path and cache the contents in the
    /// store
    fn read_proposal_state(&mut self) -> Result<(), YamlAdminStoreError> {
        let proposal_file = File::open(&self.proposal_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to open YAML proposal state file",
                Box::new(err),
            )
        })?;

        let proposals_state: ProposalState =
            serde_yaml::from_reader(&proposal_file).map_err(|err| {
                YamlAdminStoreError::general_error_with_source(
                    "Failed to read YAML proposal state file",
                    Box::new(err),
                )
            })?;

        let mut state = self.state.lock().map_err(|_| {
            YamlAdminStoreError::general_error("YAML admin service store's internal lock poisoned")
        })?;

        state.proposal_state = proposals_state;
        Ok(())
    }

    /// Read circuit state from the circuit file path and cache the contents in the store and then
    /// read circuit proposal state from the proposal file path and cache the contents in the
    /// store
    fn read_state(&mut self) -> Result<(), YamlAdminStoreError> {
        let circuit_file = File::open(&self.circuit_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to open YAML circuit state file",
                Box::new(err),
            )
        })?;

        let yaml_state_circuits: YamlCircuitState = serde_yaml::from_reader(&circuit_file)
            .map_err(|err| {
                YamlAdminStoreError::general_error_with_source(
                    "Failed to read YAML circuit state file",
                    Box::new(err),
                )
            })?;

        let yaml_state = CircuitState::from(yaml_state_circuits);

        let proposal_file = File::open(&self.proposal_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to open YAML proposal state file",
                Box::new(err),
            )
        })?;

        let proposals_state: ProposalState =
            serde_yaml::from_reader(&proposal_file).map_err(|err| {
                YamlAdminStoreError::general_error_with_source(
                    "Failed to read YAML proposal state file",
                    Box::new(err),
                )
            })?;

        let mut state = self.state.lock().map_err(|_| {
            YamlAdminStoreError::general_error("YAML admin service store's internal lock poisoned")
        })?;

        for (circuit_id, circuit) in yaml_state.circuits.iter() {
            for service in circuit.roster.iter() {
                let service_id =
                    ServiceId::new(service.service_id.to_string(), circuit_id.to_string());

                state.service_directory.insert(service_id, service.clone());
            }
        }

        state.circuit_state = yaml_state;
        state.proposal_state = proposals_state;

        Ok(())
    }

    /// Write the current circuit state to file at the circuit file path
    fn write_circuit_state(&self) -> Result<(), YamlAdminStoreError> {
        let state = self.state.lock().map_err(|_| {
            YamlAdminStoreError::general_error("YAML admin service store's internal lock poisoned")
        })?;

        let circuit_output = serde_yaml::to_vec(&YamlCircuitState::from(
            state.circuit_state.clone(),
        ))
        .map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to write circuit state to YAML",
                Box::new(err),
            )
        })?;

        let mut circuit_file = File::create(&self.circuit_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to open YAML circuit state file '{}'",
                    self.circuit_file_path
                ),
                Box::new(err),
            )
        })?;

        circuit_file.write_all(&circuit_output).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML circuit state file '{}'",
                    self.circuit_file_path
                ),
                Box::new(err),
            )
        })?;

        // Append newline to file
        writeln!(circuit_file).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML circuit file '{}'",
                    self.circuit_file_path
                ),
                Box::new(err),
            )
        })?;

        Ok(())
    }

    /// Write the current circuit proposal state to file at the proposal file path
    fn write_proposal_state(&self) -> Result<(), YamlAdminStoreError> {
        let state = self.state.lock().map_err(|_| {
            YamlAdminStoreError::general_error("YAML admin service store's internal lock poisoned")
        })?;

        let proposal_output = serde_yaml::to_vec(&state.proposal_state).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to write proposal state to YAML",
                Box::new(err),
            )
        })?;

        let mut proposal_file = File::create(&self.proposal_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to open YAML proposal state file '{}'",
                    self.proposal_file_path
                ),
                Box::new(err),
            )
        })?;

        proposal_file.write_all(&proposal_output).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML proposal state file '{}'",
                    self.proposal_file_path
                ),
                Box::new(err),
            )
        })?;

        // Append newline to file
        writeln!(proposal_file).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML proposal file '{}'",
                    self.proposal_file_path
                ),
                Box::new(err),
            )
        })?;

        Ok(())
    }

    /// Write the current circuit state to file at the circuit file path and then write the current
    /// proposal state to the file at the proposal file path
    fn write_state(&self) -> Result<(), YamlAdminStoreError> {
        let state = self.state.lock().map_err(|_| {
            YamlAdminStoreError::general_error("YAML admin service store's internal lock poisoned")
        })?;

        let circuit_output = serde_yaml::to_vec(&YamlCircuitState::from(
            state.circuit_state.clone(),
        ))
        .map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to write circuit state to YAML",
                Box::new(err),
            )
        })?;

        let mut circuit_file = File::create(&self.circuit_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to open YAML circuit state file '{}'",
                    self.circuit_file_path
                ),
                Box::new(err),
            )
        })?;

        circuit_file.write_all(&circuit_output).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML circuit state file '{}'",
                    self.circuit_file_path
                ),
                Box::new(err),
            )
        })?;

        // Append newline to file
        writeln!(circuit_file).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML circuit file '{}'",
                    self.circuit_file_path
                ),
                Box::new(err),
            )
        })?;

        let proposal_output = serde_yaml::to_vec(&state.proposal_state).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                "Failed to write proposal state to YAML",
                Box::new(err),
            )
        })?;

        let mut proposal_file = File::create(&self.proposal_file_path).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to open YAML proposal state file '{}'",
                    self.proposal_file_path
                ),
                Box::new(err),
            )
        })?;

        proposal_file.write_all(&proposal_output).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML proposal state file '{}'",
                    self.proposal_file_path
                ),
                Box::new(err),
            )
        })?;

        // Append newline to file
        writeln!(proposal_file).map_err(|err| {
            YamlAdminStoreError::general_error_with_source(
                &format!(
                    "Failed to write to YAML proposal file '{}'",
                    self.proposal_file_path
                ),
                Box::new(err),
            )
        })?;

        Ok(())
    }
}

/// Defines methods for CRUD operations and fetching and listing circuits, proposals, nodes and
/// services from a YAML file backend
impl AdminServiceStore for YamlAdminServiceStore {
    /// Adds a circuit proposal to the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `proposal` - The proposal to be added
    ///
    ///  Returns an error if a `CircuitProposal` with the same ID already exists
    fn add_proposal(&self, proposal: CircuitProposal) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;

            if state
                .proposal_state
                .proposals
                .contains_key(&proposal.circuit_id)
            {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A proposal with ID {} already exists", proposal.circuit_id),
                    source: None,
                });
            } else {
                state
                    .proposal_state
                    .proposals
                    .insert(proposal.circuit_id.to_string(), proposal);
            }
        }

        self.write_proposal_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write proposal state yaml file".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Updates a circuit proposal in the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `proposal` - The proposal with the updated information
    ///
    ///  Returns an error if a `CircuitProposal` with the same ID does not exist
    fn update_proposal(&self, proposal: CircuitProposal) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;

            if state
                .proposal_state
                .proposals
                .contains_key(&proposal.circuit_id)
            {
                state
                    .proposal_state
                    .proposals
                    .insert(proposal.circuit_id.to_string(), proposal);
            } else {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A proposal with ID {} does not exist", proposal.circuit_id),
                    source: None,
                });
            }
        }

        self.write_proposal_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write proposal state yaml file".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Removes a circuit proposal from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `proposal_id` - The unique ID of the circuit proposal to be removed
    ///
    ///  Returns an error if a `CircuitProposal` with specified ID does not exist
    fn remove_proposal(&self, proposal_id: &str) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;

            if state.proposal_state.proposals.contains_key(proposal_id) {
                state.proposal_state.proposals.remove(proposal_id);
            } else {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A proposal with ID {} does not exist", proposal_id),
                    source: None,
                });
            }
        }

        self.write_proposal_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write proposal state yaml file".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Fetches a circuit proposal from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `proposal_id` - The unique ID of the circuit proposal to be returned
    fn fetch_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<CircuitProposal>, AdminServiceStoreError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .proposal_state
            .proposals
            .get(proposal_id)
            .cloned())
    }

    /// List circuit proposals from the underlying storage
    ///
    /// The proposals returned can be filtered by provided CircuitPredicate. This enables
    /// filtering by management type and members.
    fn list_proposals(
        &self,
        predicates: &[CircuitPredicate],
    ) -> Result<Box<dyn ExactSizeIterator<Item = CircuitProposal>>, AdminServiceStoreError> {
        let mut proposals: Vec<CircuitProposal> = self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .proposal_state
            .proposals
            .iter()
            .map(|(_, proposal)| proposal.clone())
            .collect::<Vec<CircuitProposal>>();

        proposals.retain(|proposal| {
            predicates
                .iter()
                .all(|predicate| predicate.apply_to_proposals(proposal))
        });

        Ok(Box::new(proposals.into_iter()))
    }

    /// Adds a circuit to the underlying storage. Also includes the associated Services and
    /// Nodes
    ///
    /// # Arguments
    ///
    ///  * `circuit` - The circuit to be added to state
    ///  * `nodes` - A list of nodes that represent the circuit's members
    ///
    ///  Returns an error if a `Circuit` with the same ID already exists
    fn add_circuit(
        &self,
        circuit: Circuit,
        nodes: Vec<CircuitNode>,
    ) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;

            if state.circuit_state.circuits.contains_key(&circuit.id) {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A circuit with ID {} already exists", circuit.id),
                    source: None,
                });
            } else {
                for service in circuit.roster.iter() {
                    let service_id =
                        ServiceId::new(service.service_id.to_string(), circuit.id.to_string());

                    state.service_directory.insert(service_id, service.clone());
                }

                for node in nodes.into_iter() {
                    if !state.circuit_state.nodes.contains_key(&node.id) {
                        state.circuit_state.nodes.insert(node.id.to_string(), node);
                    }
                }

                state
                    .circuit_state
                    .circuits
                    .insert(circuit.id.to_string(), circuit);
            }
        }

        self.write_circuit_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write circuit state yaml file".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Updates a circuit in the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `circuit` - The circuit with the updated information
    ///
    ///  Returns an error if a `CircuitProposal` with the same ID does not exist
    fn update_circuit(&self, circuit: Circuit) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;

            if state.circuit_state.circuits.contains_key(&circuit.id) {
                state
                    .circuit_state
                    .circuits
                    .insert(circuit.id.to_string(), circuit);
            } else {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A circuit with ID {} does not exist", circuit.id),
                    source: None,
                });
            }
        }

        self.write_circuit_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write circuit state yaml file".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Removes a circuit from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `circuit_id` - The unique ID of the circuit to be removed
    ///
    ///  Returns an error if a `Circuit` with the specified ID does not exist
    fn remove_circuit(&self, circuit_id: &str) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;
            if state.circuit_state.circuits.contains_key(circuit_id) {
                let circuit = state.circuit_state.circuits.remove(circuit_id);
                if let Some(circuit) = circuit {
                    for service in circuit.roster.iter() {
                        let service_id =
                            ServiceId::new(service.service_id.to_string(), circuit_id.to_string());
                        state.service_directory.remove(&service_id);
                    }
                }
            } else {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A circuit with ID {} does not exist", circuit_id),
                    source: None,
                });
            }
        }

        self.write_circuit_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write circuit state yaml file".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Fetches a circuit from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `circuit_id` - The unique ID of the circuit to be returned
    fn fetch_circuit(&self, circuit_id: &str) -> Result<Option<Circuit>, AdminServiceStoreError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .circuit_state
            .circuits
            .get(circuit_id)
            .cloned())
    }

    /// List all circuits from the underlying storage
    ///
    /// The proposals returned can be filtered by provided CircuitPredicate. This enables
    /// filtering by management type and members.
    fn list_circuits(
        &self,
        predicates: &[CircuitPredicate],
    ) -> Result<Box<dyn ExactSizeIterator<Item = Circuit>>, AdminServiceStoreError> {
        let mut circuits: Vec<Circuit> = self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .circuit_state
            .circuits
            .iter()
            .map(|(_, circuit)| circuit.clone())
            .collect();

        circuits.retain(|circuit| {
            predicates
                .iter()
                .all(|predicate| predicate.apply_to_circuit(circuit))
        });

        Ok(Box::new(circuits.into_iter()))
    }

    /// Adds a circuit to the underlying storage based on the proposal that is already in state..
    /// Also includes the associated Services and Nodes. The associated circuit proposal for
    /// the circuit ID is also removed
    ///
    /// # Arguments
    ///
    ///  * `circuit_id` - The ID of the circuit proposal that should be converted to a circuit
    fn upgrade_proposal_to_circuit(&self, circuit_id: &str) -> Result<(), AdminServiceStoreError> {
        {
            let mut state =
                self.state
                    .lock()
                    .map_err(|_| AdminServiceStoreError::StorageError {
                        context: "YAML admin service store's internal lock was poisoned"
                            .to_string(),
                        source: None,
                    })?;

            if let Some(proposal) = state.proposal_state.proposals.remove(circuit_id) {
                let nodes = proposal.circuit.members.to_vec();
                let services = proposal.circuit.roster.to_vec();

                let circuit = Circuit::from(proposal.circuit);
                state
                    .circuit_state
                    .circuits
                    .insert(circuit.id.to_string(), circuit);

                for service in services.into_iter() {
                    let service_id =
                        ServiceId::new(service.service_id.to_string(), circuit_id.to_string());

                    state
                        .service_directory
                        .insert(service_id, Service::from(service));
                }

                for node in nodes.into_iter() {
                    if !state.circuit_state.nodes.contains_key(&node.node_id) {
                        state
                            .circuit_state
                            .nodes
                            .insert(node.node_id.to_string(), CircuitNode::from(node));
                    }
                }
            } else {
                return Err(AdminServiceStoreError::OperationError {
                    context: format!("A circuit with ID {} does not exist", circuit_id),
                    source: None,
                });
            }
        }

        self.write_state()
            .map_err(|err| AdminServiceStoreError::StorageError {
                context: "Unable to write circiut state yaml files".to_string(),
                source: Some(Box::new(err)),
            })
    }

    /// Fetches a node from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `node_id` - The unique ID of the node to be returned
    fn fetch_node(&self, node_id: &str) -> Result<Option<CircuitNode>, AdminServiceStoreError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .circuit_state
            .nodes
            .get(node_id)
            .cloned())
    }

    /// List all nodes from the underlying storage
    fn list_nodes(
        &self,
    ) -> Result<Box<dyn ExactSizeIterator<Item = CircuitNode>>, AdminServiceStoreError> {
        let nodes: Vec<CircuitNode> = self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .circuit_state
            .nodes
            .iter()
            .map(|(_, node)| node.clone())
            .collect();

        Ok(Box::new(nodes.into_iter()))
    }

    /// Fetches a service from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `service_id` - The `ServiceId` of a service made up of the circuit ID and service ID
    fn fetch_service(
        &self,
        service_id: &ServiceId,
    ) -> Result<Option<Service>, AdminServiceStoreError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .service_directory
            .get(service_id)
            .cloned())
    }

    /// List all services in a specific circuit from the underlying storage
    ///
    /// # Arguments
    ///
    ///  * `circuit_id` - The unique ID of the circuit the services belong to
    fn list_services(
        &self,
        circuit_id: &str,
    ) -> Result<Box<dyn ExactSizeIterator<Item = Service>>, AdminServiceStoreError> {
        let services: Vec<Service> = self
            .state
            .lock()
            .map_err(|_| AdminServiceStoreError::StorageError {
                context: "YAML admin service store's internal lock was poisoned".to_string(),
                source: None,
            })?
            .circuit_state
            .circuits
            .get(circuit_id)
            .ok_or(AdminServiceStoreError::OperationError {
                context: format!("Circuit {} does not exist", circuit_id),
                source: None,
            })?
            .roster
            .clone();

        Ok(Box::new(services.into_iter()))
    }
}

/// YAML file specific circuit definition. This circuit definition in the 0.4v YAML stores service
/// arguments in a map format, which differs from the definition defined in the AdminServiceStore.
/// To handle this, circuit needs to be converted to the correct format during read/write
/// operations.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
struct YamlCircuit {
    id: String,
    roster: Vec<YamlService>,
    members: Vec<String>,
    auth: AuthorizationType,
    persistence: PersistenceType,
    durability: DurabilityType,
    routes: RouteType,
    circuit_management_type: String,
}

impl From<YamlCircuit> for Circuit {
    fn from(circuit: YamlCircuit) -> Self {
        Circuit {
            id: circuit.id,
            roster: circuit.roster.into_iter().map(Service::from).collect(),
            members: circuit.members,
            auth: circuit.auth,
            persistence: circuit.persistence,
            durability: circuit.durability,
            routes: circuit.routes,
            circuit_management_type: circuit.circuit_management_type,
        }
    }
}

impl From<Circuit> for YamlCircuit {
    fn from(circuit: Circuit) -> Self {
        YamlCircuit {
            id: circuit.id,
            roster: circuit.roster.into_iter().map(YamlService::from).collect(),
            members: circuit.members,
            auth: circuit.auth,
            persistence: circuit.persistence,
            durability: circuit.durability,
            routes: circuit.routes,
            circuit_management_type: circuit.circuit_management_type,
        }
    }
}

/// YAML file specific service definition. This service definition in the 0.4v YAML stores
/// arguments in a map format, which differs from the definition defined in the AdminServiceStore.
/// To handle this, service needs to be converted to the correct format during read/write
/// operations.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
struct YamlService {
    service_id: String,
    service_type: String,
    allowed_nodes: Vec<String>,
    arguments: BTreeMap<String, String>,
}

impl From<YamlService> for Service {
    fn from(service: YamlService) -> Self {
        Service {
            service_id: service.service_id,
            service_type: service.service_type,
            allowed_nodes: service.allowed_nodes,
            arguments: service
                .arguments
                .into_iter()
                .map(|(key, value)| (key, value))
                .collect(),
        }
    }
}

impl From<Service> for YamlService {
    fn from(service: Service) -> Self {
        YamlService {
            service_id: service.service_id,
            service_type: service.service_type,
            allowed_nodes: service.allowed_nodes,
            arguments: service
                .arguments
                .into_iter()
                .map(|(key, value)| (key, value))
                .collect(),
        }
    }
}

/// YAML file specific state definition that can be read and written to the circuit YAML state file
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
struct YamlCircuitState {
    nodes: BTreeMap<String, CircuitNode>,
    circuits: BTreeMap<String, YamlCircuit>,
}

impl From<YamlCircuitState> for CircuitState {
    fn from(state: YamlCircuitState) -> Self {
        CircuitState {
            nodes: state.nodes,
            circuits: state
                .circuits
                .into_iter()
                .map(|(id, circuit)| (id, Circuit::from(circuit)))
                .collect(),
        }
    }
}

impl From<CircuitState> for YamlCircuitState {
    fn from(state: CircuitState) -> Self {
        YamlCircuitState {
            nodes: state.nodes,
            circuits: state
                .circuits
                .into_iter()
                .map(|(id, circuit)| (id, YamlCircuit::from(circuit)))
                .collect(),
        }
    }
}

/// The circuit state that is cached by the YAML admin service store and used to respond to fetch
/// requests
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
struct CircuitState {
    nodes: BTreeMap<String, CircuitNode>,
    circuits: BTreeMap<String, Circuit>,
}

/// The proposal state that is cached by the YAML admin service store and used to respond to fetch
/// requests
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
struct ProposalState {
    proposals: BTreeMap<String, CircuitProposal>,
}

/// The combination of circuit and circuit proposal state
#[derive(Debug, Clone, Default)]
struct YamlState {
    circuit_state: CircuitState,
    proposal_state: ProposalState,
    service_directory: BTreeMap<ServiceId, Service>,
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use tempdir::TempDir;

    use super::*;

    use crate::admin::store::builders::{
        CircuitBuilder, CircuitNodeBuilder, CircuitProposalBuilder, ProposedCircuitBuilder,
        ProposedNodeBuilder, ProposedServiceBuilder, ServiceBuilder,
    };
    use crate::admin::store::{ProposalType, Vote, VoteRecord};
    use crate::hex::parse_hex;

    const CIRCUIT_STATE: &[u8] = b"---
nodes:
    acme-node-000:
        id: acme-node-000
        endpoints:
          - \"tcps://splinterd-node-acme:8044\"
    bubba-node-000:
        id: bubba-node-000
        endpoints:
          - \"tcps://splinterd-node-bubba:8044\"
circuits:
    WBKLF-AAAAA:
        id: WBKLF-AAAAA
        auth: Trust
        members:
          - bubba-node-000
          - acme-node-000
        roster:
          - service_id: a000
            service_type: scabbard
            allowed_nodes:
              - acme-node-000
            arguments:
              admin_keys: '[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]'
              peer_services: '[\"a001\"]'
          - service_id: a001
            service_type: scabbard
            allowed_nodes:
              - bubba-node-000
            arguments:
              admin_keys: '[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]'
              peer_services: '[\"a000\"]'
        persistence: Any
        durability: NoDurability
        routes: Any
        circuit_management_type: gameroom";

    const PROPOSAL_STATE: &[u8] = b"---
proposals:
    WBKLF-BBBBB:
        proposal_type: Create
        circuit_id: WBKLF-BBBBB
        circuit_hash: 7ddc426972710adc0b2ecd49e89a9dd805fb9206bf516079724c887bedbcdf1d
        circuit:
            circuit_id: WBKLF-BBBBB
            roster:
            - service_id: a000
              service_type: scabbard
              allowed_nodes:
                - acme-node-000
              arguments:
                - - peer_services
                  - '[\"a001\"]'
                - - admin_keys
                  - '[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]'
            - service_id: a001
              service_type: scabbard
              allowed_nodes:
                - bubba-node-000
              arguments:
                - - peer_services
                  - '[\"a000\"]'
                - - admin_keys
                  - '[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]'
            members:
            - node_id: bubba-node-000
              endpoints:
                - \"tcps://splinterd-node-bubba:8044\"
            - node_id: acme-node-000
              endpoints:
                - \"tcps://splinterd-node-acme:8044\"
            authorization_type: Trust
            persistence: Any
            durability: NoDurability
            routes: Any
            circuit_management_type: gameroom
            application_metadata: ''
            comments: \"\"
        votes: []
        requester: 0283a14e0a17cb7f665311e9b5560f4cde2b502f17e2d03223e15d90d9318d7482
        requester_node_id: acme-node-000";

    // Validate that if the YAML state files do not exist, the YamlAdminServiceStore will create
    // the files with empty states.
    //
    // 1. Creates a empty temp directory
    // 2. Create a YAML admin service directory
    // 3. Validate that the circuit and proposals YAMLfiles were created in the temp dir.
    #[test]
    fn test_write_new_files() {
        let temp_dir = TempDir::new("test_write_new_files").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // validate the files do not exist
        assert!(!PathBuf::from(circuit_path.clone()).is_file());
        assert!(!PathBuf::from(proposals_path.clone()).is_file());

        // create YamlAdminServiceStore
        let _store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        // validate the files exist now
        assert!(PathBuf::from(circuit_path.clone()).is_file());
        assert!(PathBuf::from(proposals_path.clone()).is_file());
    }

    // Validate that the YAML admin service store can properly load circuit and proposals state
    // from existing YAML files
    //
    // 1. Creates a temp directory with existing circuit and proposals yaml files
    // 2. Create a YAML admin service directory
    // 3. Validate that the circuit and proposals can be fetched from state
    #[test]
    fn test_read_existing_files() {
        // create temp dir
        let temp_dir = TempDir::new("test_read_existing_files").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // write yaml files to temp_dir
        write_file(CIRCUIT_STATE, &circuit_path);
        write_file(PROPOSAL_STATE, &proposals_path);

        // create YamlAdminServiceStore
        let store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        assert!(store
            .fetch_proposal("WBKLF-BBBBB")
            .expect("unable to fetch proposals")
            .is_some());
        assert!(store
            .fetch_circuit("WBKLF-AAAAA")
            .expect("unable to fetch circuits")
            .is_some());
    }

    // Test the proposal CRUD operations
    //
    // 1. Setup the temp directory with existing state
    // 2. Fetch an existing proposal from state, validate proposal is returned
    // 3. Fetch an non exisitng proposal from state, validate None
    // 4. Update fetched proposal with a vote record and update, validate ok
    // 5. Call update with new proposal, validate error is returned
    // 6. Add new proposal, validate ok
    // 7. List proposal, validate both the updated original proposal and new proposal is returned
    // 8. Remove original proposal, validate okay
    // 9. Validate the proposal state YAML in the temp dir matches the expected bytes and only
    //    the new proposals
    #[test]
    fn test_proposals() {
        // create temp dir
        let temp_dir = TempDir::new("test_proposals").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // write yaml files to temp_dir
        write_file(CIRCUIT_STATE, &circuit_path);
        write_file(PROPOSAL_STATE, &proposals_path);

        // create YamlAdminServiceStore
        let store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        // fetch existing proposal from state
        let mut proposal = store
            .fetch_proposal("WBKLF-BBBBB")
            .expect("unable to fetch proposals")
            .expect("Expected proposal, got none");

        assert_eq!(proposal, create_expected_proposal());

        // fetch nonexisting proposal from state
        assert!(store
            .fetch_proposal("WBKLF-BADD")
            .expect("unable to fetch proposals")
            .is_none());

        proposal.add_vote(VoteRecord {
            public_key: parse_hex(
                "035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550",
            )
            .unwrap(),
            vote: Vote::Accept,
            voter_node_id: "bubba-node-000".into(),
        });

        store
            .update_proposal(proposal.clone())
            .expect("Unable to update proposal");

        let new_proposal = new_proposal();

        assert!(
            store.update_proposal(new_proposal.clone()).is_err(),
            "Updating new proposal should fail"
        );

        store
            .add_proposal(new_proposal.clone())
            .expect("Unable to add proposal");

        assert_eq!(
            store
                .list_proposals(&vec![])
                .expect("Unable to get list of proposals")
                .collect::<Vec<CircuitProposal>>(),
            vec![proposal, new_proposal.clone()]
        );

        store
            .remove_proposal("WBKLF-BBBBB")
            .expect("Unable to remove proposals");

        let mut yaml_state = BTreeMap::new();
        yaml_state.insert(new_proposal.circuit_id.to_string(), new_proposal);
        let mut yaml_state_vec = serde_yaml::to_vec(&ProposalState {
            proposals: yaml_state,
        })
        .unwrap();

        // Add new line because the file has a new added to it
        yaml_state_vec.append(&mut "\n".as_bytes().to_vec());

        let mut contents = vec![];
        File::open(proposals_path.clone())
            .unwrap()
            .read_to_end(&mut contents)
            .expect("Unable to read proposals");

        assert_eq!(yaml_state_vec, contents)
    }

    // Test the circuit CRUD operations
    //
    // 1. Setup the temp directory with existing state
    // 2. Fetch an existing circuit from state, validate circuit is returned
    // 3. Fetch an non exisitng circuit from state, validate None
    // 4. Update fetched proposa with a vote record and update, validate ok
    // 5. Call update with new circuit, validate error is returned
    // 6. Add new circuit, validate ok
    // 7. List circuit, validate both the updated original circuit and new circuit is returned
    // 8. Remove original circuit, validate okay
    // 9. Validate the circuit state YAML in the temp dir matches the expected bytes and contains
    //    only the new circuit
    #[test]
    fn test_circuit() {
        // create temp dir
        let temp_dir = TempDir::new("test_circuit").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // write yaml files to temp_dir
        write_file(CIRCUIT_STATE, &circuit_path);
        write_file(PROPOSAL_STATE, &proposals_path);

        // create YamlAdminServiceStore
        let store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        // fetch existing circuit from state
        let mut circuit = store
            .fetch_circuit("WBKLF-AAAAA")
            .expect("unable to fetch circuit")
            .expect("Expected circuit, got none");

        assert_eq!(circuit, create_expected_circuit());

        // fetch nonexisting circuitfrom state
        assert!(store
            .fetch_circuit("WBKLF-BADD")
            .expect("unable to fetch circuit")
            .is_none());

        circuit.circuit_management_type = "test".to_string();

        store
            .update_circuit(circuit.clone())
            .expect("Unable to update circuit");

        let (new_circuit, new_node) = new_circuit();

        assert!(
            store.update_circuit(new_circuit.clone()).is_err(),
            "Updating new cirucit should fail"
        );

        store
            .add_circuit(new_circuit.clone(), vec![new_node.clone()])
            .expect("Unable to add cirucit");

        assert_eq!(
            store
                .list_circuits(&vec![])
                .expect("Unable to get list of circuits")
                .collect::<Vec<Circuit>>(),
            vec![circuit, new_circuit.clone()]
        );

        store
            .remove_circuit("WBKLF-AAAAA")
            .expect("Unable to remove circuit");

        let mut yaml_circuits = BTreeMap::new();
        let mut yaml_nodes = BTreeMap::new();
        yaml_circuits.insert(new_circuit.id.to_string(), YamlCircuit::from(new_circuit));
        yaml_nodes.insert(
            "acme-node-000".to_string(),
            CircuitNode {
                id: "acme-node-000".to_string(),
                endpoints: vec!["tcps://splinterd-node-acme:8044".into()],
            },
        );
        yaml_nodes.insert(
            "bubba-node-000".to_string(),
            CircuitNode {
                id: "bubba-node-000".to_string(),
                endpoints: vec!["tcps://splinterd-node-bubba:8044".into()],
            },
        );
        yaml_nodes.insert(new_node.id.to_string(), new_node);
        let mut yaml_state_vec = serde_yaml::to_vec(&YamlCircuitState {
            circuits: yaml_circuits,
            nodes: yaml_nodes,
        })
        .unwrap();

        // Add new line because the file has a new added to it
        yaml_state_vec.append(&mut "\n".as_bytes().to_vec());

        let mut contents = vec![];
        File::open(circuit_path.clone())
            .unwrap()
            .read_to_end(&mut contents)
            .expect("Unable to read proposals");

        assert_eq!(yaml_state_vec, contents)
    }

    // Test the node CRUD operations
    //
    // 1. Setup the temp directory with existing state
    // 2. Check that the expected node is returned when fetched
    // 3. Check that the expected nodes are returned when list_nodes is called
    #[test]
    fn test_node() {
        // create temp dir
        let temp_dir = TempDir::new("test_node").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // write yaml files to temp_dir
        write_file(CIRCUIT_STATE, &circuit_path);
        write_file(PROPOSAL_STATE, &proposals_path);

        // create YamlAdminServiceStore
        let store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        let node = store
            .fetch_node("acme-node-000")
            .expect("Unable to fetch node")
            .expect("expected node, got none");

        assert_eq!(
            node,
            CircuitNode {
                id: "acme-node-000".to_string(),
                endpoints: vec!["tcps://splinterd-node-acme:8044".into()],
            }
        );

        assert_eq!(
            store.list_nodes().unwrap().collect::<Vec<CircuitNode>>(),
            vec![
                CircuitNode {
                    id: "acme-node-000".to_string(),
                    endpoints: vec!["tcps://splinterd-node-acme:8044".into()],
                },
                CircuitNode {
                    id: "bubba-node-000".to_string(),
                    endpoints: vec!["tcps://splinterd-node-bubba:8044".into()],
                }
            ]
        );
    }

    // Test the service CRUD operations
    //
    // 1. Setup the temp directory with existing state
    // 2. Check that the expected service is returned when fetched
    // 3. Check that the expected services are returned when list_services is called
    #[test]
    fn test_service() {
        // create temp dir
        let temp_dir = TempDir::new("test_service").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // write yaml files to temp_dir
        write_file(CIRCUIT_STATE, &circuit_path);
        write_file(PROPOSAL_STATE, &proposals_path);

        let service_id = ServiceId::new("a000".to_string(), "WBKLF-AAAAA".to_string());

        // create YamlAdminServiceStore
        let store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        let service = store
            .fetch_service(&service_id)
            .expect("Unable to fetch service")
            .expect("unable to get expected service, got none");

        assert_eq!(
            service,
            ServiceBuilder::default()
                .with_service_id("a000")
                .with_service_type("scabbard")
                .with_allowed_nodes(&vec!["acme-node-000".into()])
                .with_arguments(&vec![
                    (
                        "admin_keys".into(),
                        "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]"
                            .into()
                    ),
                    ("peer_services".into(), "[\"a001\"]".into()),
                ])
                .build()
                .expect("Unable to build service"),
        );

        assert_eq!(
            store
                .list_services("WBKLF-AAAAA")
                .unwrap()
                .collect::<Vec<Service>>(),
            vec![
                ServiceBuilder::default()
                    .with_service_id("a000")
                    .with_service_type("scabbard")
                    .with_allowed_nodes(&vec!["acme-node-000".into()])
                    .with_arguments(&vec![
                    ("admin_keys".into(),
                   "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]"
                   .into()),
                   ("peer_services".into(), "[\"a001\"]".into()),
                ])
                    .build()
                    .expect("Unable to build service"),
                ServiceBuilder::default()
                    .with_service_id("a001")
                    .with_service_type("scabbard")
                    .with_allowed_nodes(&vec!["bubba-node-000".into()])
                    .with_arguments(&vec![
                        ("admin_keys".into(),
                       "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]"
                       .into()),
                           ("peer_services".into(), "[\"a000\"]".into()),
                    ])
                    .build()
                    .expect("Unable to build service")
            ]
        );
    }

    // Test that a proposals can be upgraded to a circuit and both yaml files are upgraded.
    //
    // 1. Setup the temp directory with existing proposal state
    // 2. Upgrade proposal to circuit, validate ok
    // 3. Check that proposals are now empty
    // 4. Check that the circuit, nodes and services have been set
    #[test]
    fn test_upgrading_proposals_to_circuit() {
        // create temp dir
        let temp_dir =
            TempDir::new("est_upgrading_proposals_to_circuit").expect("Failed to create temp dir");
        let circuit_path = temp_dir
            .path()
            .join("circuits.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        let proposals_path = temp_dir
            .path()
            .join("circuit_proposals.yaml")
            .to_str()
            .expect("Failed to get path")
            .to_string();

        // write proposal to state
        write_file(PROPOSAL_STATE, &proposals_path);

        // create YamlAdminServiceStore
        let store = YamlAdminServiceStore::new(circuit_path.clone(), proposals_path.clone())
            .expect("Unable to create yaml admin store");

        let service_id = ServiceId::new("a000".to_string(), "WBKLF-BBBBB".to_string());
        assert_eq!(store.fetch_circuit("WBKLF-BBBBB").unwrap(), None);
        assert_eq!(store.fetch_node("acme-node-000").unwrap(), None);
        assert_eq!(store.fetch_service(&service_id).unwrap(), None);

        store
            .upgrade_proposal_to_circuit("WBKLF-BBBBB")
            .expect("Unable to upgrade proposalto circuit");

        assert_eq!(store.list_proposals(&vec![]).unwrap().next(), None);

        assert!(store.fetch_circuit("WBKLF-BBBBB").unwrap().is_some());
        assert!(store.fetch_node("acme-node-000").unwrap().is_some());
        assert!(store.fetch_service(&service_id).unwrap().is_some());
    }

    fn write_file(data: &[u8], file_path: &str) {
        let mut file = File::create(file_path).expect("Error creating test yaml file.");
        file.write_all(data)
            .expect("unable to write test file to temp dir")
    }

    fn create_expected_proposal() -> CircuitProposal {
        CircuitProposalBuilder::default()
            .with_proposal_type(&ProposalType::Create)
            .with_circuit_id("WBKLF-BBBBB")
            .with_circuit_hash(
                "7ddc426972710adc0b2ecd49e89a9dd805fb9206bf516079724c887bedbcdf1d")
            .with_circuit(
                &ProposedCircuitBuilder::default()
                    .with_circuit_id("WBKLF-BBBBB")
                    .with_roster(&vec![
                        ProposedServiceBuilder::default()
                            .with_service_id("a000")
                            .with_service_type("scabbard")
                            .with_allowed_nodes(&vec!["acme-node-000".into()])
                            .with_arguments(&vec![
                                ("peer_services".into(), "[\"a001\"]".into()),
                                ("admin_keys".into(),
                               "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]".into())
                            ])
                            .build().expect("Unable to build service"),
                        ProposedServiceBuilder::default()
                            .with_service_id("a001")
                            .with_service_type("scabbard")
                            .with_allowed_nodes(&vec!["bubba-node-000".into()])
                            .with_arguments(&vec![
                                ("peer_services".into(), "[\"a000\"]".into()),
                                ("admin_keys".into(),
                               "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]".into())
                            ])
                            .build().expect("Unable to build service")
                        ])

                    .with_members(
                        &vec![
                        ProposedNodeBuilder::default()
                            .with_node_id("bubba-node-000".into())
                            .with_endpoints(&vec!["tcps://splinterd-node-bubba:8044".into()])
                            .build().expect("Unable to build node"),
                        ProposedNodeBuilder::default()
                            .with_node_id("acme-node-000".into())
                            .with_endpoints(&vec!["tcps://splinterd-node-acme:8044".into()])
                            .build().expect("Unable to build node"),
                        ]
                    )
                    .with_circuit_management_type("gameroom")
                    .build().expect("Unable to build circuit")
            )
            .with_requester(
                &parse_hex(
                    "0283a14e0a17cb7f665311e9b5560f4cde2b502f17e2d03223e15d90d9318d7482").unwrap())
            .with_requester_node_id("acme-node-000")
            .build().expect("Unable to build proposals")
    }

    fn create_expected_circuit() -> Circuit {
        CircuitBuilder::default()
            .with_circuit_id("WBKLF-AAAAA")
            .with_roster(&vec![
                ServiceBuilder::default()
                    .with_service_id("a000")
                    .with_service_type("scabbard")
                    .with_allowed_nodes(&vec!["acme-node-000".into()])
                    .with_arguments(&vec![
                        ("admin_keys".into(),
                       "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]"
                            .into()),
                       ("peer_services".into(), "[\"a001\"]".into()),
                    ])
                    .build()
                    .expect("Unable to build service"),
                ServiceBuilder::default()
                    .with_service_id("a001")
                    .with_service_type("scabbard")
                    .with_allowed_nodes(&vec!["bubba-node-000".into()])
                    .with_arguments(&vec![(
                        "admin_keys".into(),
                        "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]"
                            .into()
                    ),(
                        "peer_services".into(), "[\"a000\"]".into()
                    )])
                    .build()
                    .expect("Unable to build service"),
            ])
            .with_members(&vec!["bubba-node-000".into(), "acme-node-000".into()])
            .with_circuit_management_type("gameroom")
            .build()
            .expect("Unable to build circuit")
    }

    fn new_proposal() -> CircuitProposal {
        CircuitProposalBuilder::default()
            .with_proposal_type(&ProposalType::Create)
            .with_circuit_id("WBKLF-CCCCC")
            .with_circuit_hash(
                "7ddc426972710adc0b2ecd49e89a9dd805fb9206bf516079724c887bedbcdf1d")
            .with_circuit(
                &ProposedCircuitBuilder::default()
                    .with_circuit_id("WBKLF-PqfoE")
                    .with_roster(&vec![
                        ProposedServiceBuilder::default()
                            .with_service_id("a000")
                            .with_service_type("scabbard")
                            .with_allowed_nodes(&vec!["acme-node-000".into()])
                            .with_arguments(&vec![
                                ("peer_services".into(), "[\"a001\"]".into()),
                                ("admin_keys".into(),
                               "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]".into())
                            ])
                            .build().expect("Unable to build service"),
                        ProposedServiceBuilder::default()
                            .with_service_id("a001")
                            .with_service_type("scabbard")
                            .with_allowed_nodes(&vec!["bubba-node-000".into()])
                            .with_arguments(&vec![
                                ("peer_services".into(), "[\"a000\"]".into()),
                                ("admin_keys".into(),
                               "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]".into())
                            ])
                            .build().expect("Unable to build service")
                        ])

                    .with_members(
                        &vec![
                        ProposedNodeBuilder::default()
                            .with_node_id("bubba-node-000".into())
                            .with_endpoints(&vec!["tcps://splinterd-node-bubba:8044".into()])
                            .build().expect("Unable to build node"),
                        ProposedNodeBuilder::default()
                            .with_node_id("acme-node-000".into())
                            .with_endpoints(&vec!["tcps://splinterd-node-acme:8044".into()])
                            .build().expect("Unable to build node"),
                        ]
                    )
                    .with_circuit_management_type("test")
                    .build().expect("Unable to build circuit")
            )
            .with_requester(
                &parse_hex(
                    "0283a14e0a17cb7f665311e9b5560f4cde2b502f17e2d03223e15d90d9318d7482").unwrap())
            .with_requester_node_id("acme-node-000")
            .build().expect("Unable to build proposals")
    }

    fn new_circuit() -> (Circuit, CircuitNode) {
        (CircuitBuilder::default()
            .with_circuit_id("WBKLF-DDDDD")
            .with_roster(&vec![
                ServiceBuilder::default()
                    .with_service_id("a000")
                    .with_service_type("scabbard")
                    .with_allowed_nodes(&vec!["acme-node-000".into()])
                    .with_arguments(&vec![
                        ("peer_services".into(), "[\"a001\"]".into()),
                        ("admin_keys".into(),
                       "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]".into())
                    ])
                    .build().expect("Unable to build service"),
                ServiceBuilder::default()
                    .with_service_id("a001")
                    .with_service_type("scabbard")
                    .with_allowed_nodes(&vec!["bubba-node-000".into()])
                    .with_arguments(&vec![
                        ("peer_services".into(), "[\"a000\"]".into()),
                        ("admin_keys".into(),
                       "[\"035724d11cae47c8907f8bfdf510488f49df8494ff81b63825bad923733c4ac550\"]".into())
                    ])
                    .build().expect("Unable to build service")
                ])
            .with_members(
                &vec![
                    "bubba-node-000".into(),
                    "acme-node-000".into(),
                    "new-node-000".into()
                ]
            )
            .with_circuit_management_type("test")
            .build().expect("Unable to build circuit"),
        CircuitNodeBuilder::default()
            .with_node_id("new-node-000".into())
            .with_endpoints(&vec!["tcps://splinterd-node-new:8044".into()])
            .build().expect("Unable to build node"))
    }
}
