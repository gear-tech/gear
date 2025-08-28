pub use gbuiltin_bls381::{Request, Response};

use super::BuiltinActorError;
use ark_bls12_381::{G1Projective as G1, G2Affine, G2Projective as G2};
use ark_ec::{
    bls12::Bls12Config,
    hashing::{HashToCurve, curve_maps::wb, map_to_curve_hasher::MapToCurveBasedHasher},
};
use ark_ff::fields::field_hashers::DefaultFieldHasher;
use ark_scale::{ArkScale, HOST_CALL};
use ark_serialize::{CanonicalDeserialize, Compress, Validate};
use gear_core::{ids::ActorId, message::StoredDispatch, str::LimitedStr};
use parity_scale_codec::{Compact, Decode, Encode, Input};
use scale_info::TypeInfo;
use sp_crypto_ec_utils::bls12_381;

/// The id of the BLS12-381 builtin actor.
pub const BLS12_381_ID: ActorId = ActorId::new(*b"modl/bia/bls12-381/v-\x01\0/\0\0\0\0\0\0\0\0");
const IS_COMPRESSED: Compress = ark_scale::is_compressed(HOST_CALL);
const IS_VALIDATED: Validate = ark_scale::is_validated(HOST_CALL);

pub fn process_bls12_381_dispatch(mut payload: &[u8]) -> Result<Response, BuiltinActorError> {
    let payload_decoded =
        Request::decode(&mut payload).map_err(|_| BuiltinActorError::DecodingError)?;

    match payload_decoded {
        Request::MultiMillerLoop { a, b } => multi_miller_loop(a, b),
        _ => todo!(),
    }
}

fn multi_miller_loop(a: Vec<u8>, b: Vec<u8>) -> Result<Response, BuiltinActorError> {
    // decode the items count
    let mut slice = a.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!("Failed to decode items count in a");

        return Err(BuiltinActorError::DecodingError);
    };

    let mut slice = b.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi Miller loop: uneven item count",
            )));
        }
        Err(_) => return Err(BuiltinActorError::DecodingError),
        Ok(_) => (),
    }

    // todo [sab] charge gas
    // let to_spend = WeightInfo::bls12_381_multi_miller_loop(count as u32).ref_time();
    // context.try_charge_gas(to_spend)?;

    match bls12_381::host_calls::bls12_381_multi_miller_loop(a, b) {
        Ok(result) => Ok(Response::MultiMillerLoop(result)),
        Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Multi Miller loop: computation error",
        ))),
    }
}

fn final_exponentiation(mut payload: &[u8]) -> Result<Response, BuiltinActorError> {
    let f = decode_vec(&mut payload)?;

    // todo [sab] charge gas
    // let to_spend = WeightInfo::bls12_381_final_exponentiation().ref_time();
    // context.try_charge_gas(to_spend)?;

    match bls12_381::host_calls::bls12_381_final_exponentiation(f) {
        Ok(result) => Ok(Response::FinalExponentiation(result)),
        Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Final exponentiation: computation error",
        ))),
    }
}

fn msm_g1(payload: &[u8]) -> Result<Response, BuiltinActorError> {
    msm(
        payload,
        |count| 0, // WeightInfo::bls12_381_msm_g1(count).ref_time(),
        |bases, scalars| {
            bls12_381::host_calls::bls12_381_msm_g1(bases, scalars)
                .map(Response::MultiScalarMultiplicationG1)
        },
    )
}

fn msm_g2(payload: &[u8]) -> Result<Response, BuiltinActorError> {
    msm(
        payload,
        |count| 0, // WeightInfo::bls12_381_msm_g2(count).ref_time(),
        |bases, scalars| {
            bls12_381::host_calls::bls12_381_msm_g2(bases, scalars)
                .map(Response::MultiScalarMultiplicationG2)
        },
    )
}

fn msm(
    mut payload: &[u8],
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, ()>,
) -> Result<Response, BuiltinActorError> {
    let bases = decode_vec(&mut payload)?;
    let scalars = decode_vec(&mut payload)?;

    // decode the count of items
    let mut slice = bases.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!("Failed to decode items count in bases");

        return Err(BuiltinActorError::DecodingError);
    };

    let mut slice = scalars.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    match u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) {
        Ok(count_b) if count_b != count => {
            return Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
                "Multi scalar multiplication: uneven item count",
            )));
        }
        Err(_) => {
            log::debug!("Failed to decode items count in scalars");

            return Err(BuiltinActorError::DecodingError);
        }
        Ok(_) => (),
    }

    // todo [sab] charge gas
    // let to_spend = gas_to_spend(count as u32);
    // context.try_charge_gas(to_spend)?;

    match call(bases, scalars) {
        Ok(result) => Ok(result),
        Err(_) => Err(BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Multi scalar multiplication: computation error",
        ))),
    }
}

fn projective_multiplication_g1(payload: &[u8]) -> Result<Response, BuiltinActorError> {
    projective_multiplication(
        payload,
        |count| 0, // WeightInfo::bls12_381_mul_projective_g1(count).ref_time(),
        |base, scalar| {
            bls12_381::host_calls::bls12_381_mul_projective_g1(base, scalar)
                .map(Response::ProjectiveMultiplicationG1)
        },
    )
}

fn projective_multiplication_g2(payload: &[u8]) -> Result<Response, BuiltinActorError> {
    projective_multiplication(
        payload,
        |count| 0, // WeightInfo::bls12_381_mul_projective_g2(count).ref_time(),
        |base, scalar| {
            bls12_381::host_calls::bls12_381_mul_projective_g2(base, scalar)
                .map(Response::ProjectiveMultiplicationG2)
        },
    )
}

fn projective_multiplication(
    mut payload: &[u8],
    gas_to_spend: impl FnOnce(u32) -> u64,
    call: impl FnOnce(Vec<u8>, Vec<u8>) -> Result<Response, ()>,
) -> Result<Response, BuiltinActorError> {
    let base = decode_vec(&mut payload)?;
    let scalar = decode_vec(&mut payload)?;

    // decode the count of items
    let mut slice = scalar.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!("Failed to decode items count in scalar");

        return Err(BuiltinActorError::DecodingError);
    };

    // todo [sab] charge gas
    // let to_spend = gas_to_spend(count as u32);
    // context.try_charge_gas(to_spend)?;

    call(base, scalar).map_err(|_| {
        BuiltinActorError::Custom(LimitedStr::from_small_str(
            "Projective multiplication: computation error",
        ))
    })
}

fn aggregate_g1(mut payload: &[u8]) -> Result<Response, BuiltinActorError> {
    let points = decode_vec(&mut payload)?;

    // decode the count of items
    let mut slice = points.as_slice();
    let mut reader = ark_scale::rw::InputAsRead(&mut slice);
    let Ok(count) = u64::deserialize_with_mode(&mut reader, IS_COMPRESSED, IS_VALIDATED) else {
        log::debug!("Failed to decode items count in points");

        return Err(BuiltinActorError::DecodingError);
    };

    // todo [sab] charge gas
    // let to_spend = WeightInfo::bls12_381_aggregate_g1(count as u32).ref_time();
    // context.try_charge_gas(to_spend)?;

    aggregate_g1_impl(&points)
        .map(Response::AggregateG1)
        .inspect_err(|e| {
            log::debug!("Failed to aggregate G1-points: {e:?}");
        })
}

fn aggregate_g1_impl(points: &[u8]) -> Result<Vec<u8>, BuiltinActorError> {
    type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

    let ArkScale(points) = ArkScale::<Vec<G1>>::decode(&mut &points[..])
        .map_err(|_| BuiltinActorError::DecodingError)?;

    let point_first = points.first().ok_or(BuiltinActorError::EmptyPointList)?;

    let point_aggregated = points
        .iter()
        .skip(1)
        .fold(*point_first, |aggregated, point| aggregated + *point);

    Ok(ArkScale::<G1>::from(point_aggregated).encode())
}

fn map_to_g2affine(mut payload: &[u8]) -> Result<Response, BuiltinActorError> {
    let len = Compact::<u32>::decode(&mut payload)
        .map(u32::from)
        .map_err(|_| {
            log::debug!("Failed to scale-decode vector length");
            BuiltinActorError::DecodingError
        })?;

    if len != payload.len() as u32 {
        log::debug!("Failed to scale-decode vector length");

        return Err(BuiltinActorError::DecodingError);
    }

    // todo [sab] charge gas
    // let to_spend = WeightInfo::bls12_381_map_to_g2affine(len).ref_time();
    // context.try_charge_gas(to_spend)?;

    map_to_g2affine_impl(payload)
        .map(Response::MapToG2Affine)
        .inspect_err(|e| {
            log::debug!("Failed to map a message: {e:?}");
        })
}

fn map_to_g2affine_impl(message: &[u8]) -> Result<Vec<u8>, BuiltinActorError> {
    type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;
    type WBMap = wb::WBMap<<ark_bls12_381::Config as Bls12Config>::G2Config>;

    const DST_G2: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

    let mapper = MapToCurveBasedHasher::<G2, DefaultFieldHasher<sha2::Sha256>, WBMap>::new(DST_G2)
        .map_err(|_| BuiltinActorError::MapperCreationError)?;

    let point = mapper
        .hash(message)
        .map_err(|_| BuiltinActorError::MessageMappingError)?;

    Ok(ArkScale::<G2Affine>::from(point).encode())
}

fn decode_vec<I: Input>(input: &mut I) -> Result<Vec<u8>, BuiltinActorError> {
    let len = Compact::<u32>::decode(input).map(u32::from).map_err(|_| {
        log::debug!("Failed to scale-decode vector length");
        BuiltinActorError::DecodingError
    })?;

    // todo [sab] charge gas
    // let to_spend = WeightInfo::decode_bytes(len).ref_time();
    // context.try_charge_gas(to_spend)?;

    let mut items = vec![0u8; len as usize];
    let bytes_slice = items.as_mut_slice();

    input.read(bytes_slice).map(|_| items).map_err(|_| {
        log::debug!("Failed to scale-decode vector data");

        BuiltinActorError::DecodingError
    })
}
