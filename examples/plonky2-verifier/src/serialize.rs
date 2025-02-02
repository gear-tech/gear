// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Plonky2 circuit data and proof serialization.

use plonky2::{
    field::extension::Extendable,
    hash::hash_types::RichField,
    plonk::{
        circuit_data::VerifierCircuitData, config::GenericConfig, proof::ProofWithPublicInputs,
    },
    util::serialization::{Buffer, DefaultGateSerializer, Read},
};

type CircuitDataAndProof<F, C, const D: usize> =
    (VerifierCircuitData<F, C, D>, ProofWithPublicInputs<F, C, D>);

pub fn parse_circuit_data_and_proof<F, C, const D: usize>(
    data: &[u8],
) -> Result<CircuitDataAndProof<F, C, D>, &'static str>
where
    F: RichField + Extendable<D>,
    C: GenericConfig<D, F = F>,
{
    let gate_serializer = DefaultGateSerializer;
    let mut buffer = Buffer::new(data);
    let verifier_circuit_data = buffer
        .read_verifier_circuit_data(&gate_serializer)
        .map_err(|_| "Common circuit data parsing error")?;
    let proof_with_pis = buffer
        .read_proof_with_public_inputs(&verifier_circuit_data.common)
        .map_err(|_| "Proof with public inputs parsing error")?;
    Ok((verifier_circuit_data, proof_with_pis))
}
