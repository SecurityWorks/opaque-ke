// Copyright (c) Meta Platforms, Inc. and affiliates.
//
// This source code is dual-licensed under either the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree or the Apache
// License, Version 2.0 found in the LICENSE-APACHE file in the root directory
// of this source tree. You may select, at your option, one of the above-listed
// licenses.

//! Contains the messages used for OPAQUE

use core::ops::Add;

use derive_where::derive_where;
use digest::Output;
use generic_array::sequence::Concat;
use generic_array::typenum::{Sum, Unsigned};
use generic_array::{ArrayLength, GenericArray};
use rand::{CryptoRng, RngCore};
use voprf::{BlindedElement, BlindedElementLen, EvaluationElement, EvaluationElementLen};
use zeroize::Zeroizing;

use crate::ciphersuite::{CipherSuite, KeGroup, OprfGroup, OprfHash};
use crate::envelope::{Envelope, EnvelopeLen};
use crate::errors::ProtocolError;
use crate::hash::OutputSize;
use crate::key_exchange::group::Group;
use crate::key_exchange::shared::NonceLen;
use crate::key_exchange::{
    Deserialize, Ke1MessageLen, Ke2MessageLen, Ke3MessageLen, KeyExchange, Serialize,
    SerializedCredentialRequest, SerializedCredentialResponse,
};
use crate::keypair::PublicKey;
use crate::opaque::{
    MaskedResponse, MaskedResponseLen, ServerLogin, ServerLoginStartResult, ServerSetup,
};
use crate::serialization::SliceExt;

////////////////////////////
// High-level API Structs //
// ====================== //
////////////////////////////

/// The message sent by the client to the server, to initiate registration
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound = "")
)]
#[derive_where(Clone)]
#[derive_where(Debug, Eq, Hash, Ord, PartialEq, PartialOrd; voprf::BlindedElement<CS::OprfCs>)]
pub struct RegistrationRequest<CS: CipherSuite> {
    /// blinded password information
    pub(crate) blinded_element: voprf::BlindedElement<CS::OprfCs>,
}

/// The answer sent by the server to the user, upon reception of the
/// registration attempt
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound = "")
)]
#[derive_where(Clone)]
#[derive_where(Debug, Eq, Hash, Ord, PartialEq, PartialOrd; voprf::EvaluationElement<CS::OprfCs>, <KeGroup<CS> as Group>::Pk)]
pub struct RegistrationResponse<CS: CipherSuite> {
    /// The server's oprf output
    pub(crate) evaluation_element: voprf::EvaluationElement<CS::OprfCs>,
    /// Server's static public key
    pub(crate) server_s_pk: PublicKey<KeGroup<CS>>,
}

/// The final message from the client, containing sealed cryptographic
/// identifiers
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound = "")
)]
#[derive_where(Clone, ZeroizeOnDrop)]
#[derive_where(Debug, Eq, Hash, Ord, PartialEq, PartialOrd; <KeGroup<CS> as Group>::Pk)]
pub struct RegistrationUpload<CS: CipherSuite> {
    /// The "envelope" generated by the user, containing sealed cryptographic
    /// identifiers
    pub(crate) envelope: Envelope<CS>,
    /// The masking key used to mask the envelope
    pub(crate) masking_key: Output<OprfHash<CS>>,
    /// The user's public key
    pub(crate) client_s_pk: PublicKey<KeGroup<CS>>,
}

/// The message sent by the user to the server, to initiate registration
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound(
        deserialize = "<CS::KeyExchange as KeyExchange>::KE1Message: serde::Deserialize<'de>",
        serialize = "<CS::KeyExchange as KeyExchange>::KE1Message: serde::Serialize"
    ))
)]
#[derive_where(Clone, ZeroizeOnDrop)]
#[derive_where(
    Debug, Eq, Hash, PartialEq;
    voprf::BlindedElement<CS::OprfCs>,
    <CS::KeyExchange as KeyExchange>::KE1Message,
)]
pub struct CredentialRequest<CS: CipherSuite> {
    pub(crate) blinded_element: voprf::BlindedElement<CS::OprfCs>,
    pub(crate) ke1_message: <CS::KeyExchange as KeyExchange>::KE1Message,
}

/// Builder for [`ServerLogin`](crate::ServerLogin) when using remote keys.
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound(
        deserialize = "SK: serde::Deserialize<'de>, <CS::KeyExchange as \
                       KeyExchange>::KE2Builder<'a, CS>: serde::Deserialize<'de>",
        serialize = "SK: serde::Serialize, <CS::KeyExchange as KeyExchange>::KE2Builder<'a, CS>: \
                     serde::Serialize"
    ))
)]
#[derive_where(Clone)]
#[derive_where(
    Debug, Eq, PartialEq;
    <KeGroup<CS> as Group>::Pk,
    SK,
    voprf::EvaluationElement<CS::OprfCs>,
    <CS::KeyExchange as KeyExchange>::KE2Builder<'a, CS>,
)]
pub struct ServerLoginBuilder<'a, CS: CipherSuite, SK: Clone> {
    pub(crate) server_s_sk: SK,
    pub(crate) evaluation_element: voprf::EvaluationElement<CS::OprfCs>,
    pub(crate) masking_nonce: Zeroizing<GenericArray<u8, NonceLen>>,
    pub(crate) masked_response: MaskedResponse<CS>,
    #[cfg(test)]
    pub(crate) oprf_key: Zeroizing<GenericArray<u8, <OprfGroup<CS> as voprf::Group>::ScalarLen>>,
    pub(crate) ke2_builder: <CS::KeyExchange as KeyExchange>::KE2Builder<'a, CS>,
}

impl<CS: CipherSuite, SK: Clone> ServerLoginBuilder<'_, CS, SK> {
    /// The returned data here has to be processed and the result given as an
    /// input to [`ServerLoginBuilder::build()`]. To understand what kind of
    /// output is expected here and how to process it, refer to the
    /// documentation of your chosen [`CipherSuite::KeyExchange`].
    pub fn data(&self) -> <CS::KeyExchange as KeyExchange>::KE2BuilderData<'_, CS> {
        CS::KeyExchange::ke2_builder_data(&self.ke2_builder)
    }

    /// The handle to the corresponding [`ServerSetup`]s private key.
    pub fn private_key(&self) -> &SK {
        &self.server_s_sk
    }

    /// Build [`ServerLogin`] after attaining the input for the key exchange. To
    /// understand what kind of input is expected here, refer to the
    /// documentation of your chosen [`CipherSuite::KeyExchange`].
    ///
    /// See [`ServerLogin::start()`] for the regular path.
    pub fn build(
        self,
        input: <CS::KeyExchange as KeyExchange>::KE2BuilderInput<CS>,
    ) -> Result<ServerLoginStartResult<CS>, ProtocolError> {
        ServerLogin::build(self, input)
    }
}

/// The answer sent by the server to the user, upon reception of the login
/// attempt
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound(
        deserialize = "<CS::KeyExchange as KeyExchange>::KE2Message: serde::Deserialize<'de>",
        serialize = "<CS::KeyExchange as KeyExchange>::KE2Message: serde::Serialize"
    ))
)]
#[derive_where(Clone)]
#[derive_where(
    Debug, Eq, Hash, PartialEq;
    voprf::EvaluationElement<CS::OprfCs>,
    <CS::KeyExchange as KeyExchange>::KE2Message,
)]
pub struct CredentialResponse<CS: CipherSuite> {
    /// the server's oprf output
    pub(crate) evaluation_element: voprf::EvaluationElement<CS::OprfCs>,
    pub(crate) masking_nonce: GenericArray<u8, NonceLen>,
    pub(crate) masked_response: MaskedResponse<CS>,
    pub(crate) ke2_message: <CS::KeyExchange as KeyExchange>::KE2Message,
}

/// The answer sent by the client to the server, upon reception of the sealed
/// envelope
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(bound(
        deserialize = "<CS::KeyExchange as KeyExchange>::KE3Message: serde::Deserialize<'de>",
        serialize = "<CS::KeyExchange as KeyExchange>::KE3Message: serde::Serialize"
    ))
)]
#[derive_where(Clone)]
#[derive_where(
    Debug, Eq, Hash, PartialEq;
    <CS::KeyExchange as KeyExchange>::KE3Message,
)]
pub struct CredentialFinalization<CS: CipherSuite> {
    pub(crate) ke3_message: <CS::KeyExchange as KeyExchange>::KE3Message,
}

////////////////////////////////
// High-level Implementations //
// ========================== //
////////////////////////////////

/// Length of [`RegistrationRequest`] in bytes for serialization.
pub type RegistrationRequestLen<CS: CipherSuite> = <OprfGroup<CS> as voprf::Group>::ElemLen;

impl<CS: CipherSuite> RegistrationRequest<CS> {
    /// Only used for testing purposes
    #[cfg(test)]
    pub(crate) fn get_blinded_element_for_testing(&self) -> voprf::BlindedElement<CS::OprfCs> {
        self.blinded_element.clone()
    }

    /// Serialization into bytes
    pub fn serialize(&self) -> GenericArray<u8, RegistrationRequestLen<CS>> {
        <OprfGroup<CS> as voprf::Group>::serialize_elem(self.blinded_element.value())
    }

    /// Deserialization from bytes
    pub fn deserialize(input: &[u8]) -> Result<Self, ProtocolError> {
        Ok(Self {
            blinded_element: voprf::BlindedElement::deserialize(input)?,
        })
    }
}

/// Length of [`RegistrationResponse`] in bytes for serialization.
pub type RegistrationResponseLen<CS: CipherSuite> =
    Sum<<OprfGroup<CS> as voprf::Group>::ElemLen, <KeGroup<CS> as Group>::PkLen>;

impl<CS: CipherSuite> RegistrationResponse<CS> {
    /// Serialization into bytes
    pub fn serialize(&self) -> GenericArray<u8, RegistrationResponseLen<CS>>
    where
        // RegistrationResponse: KgPk + KePk
        <OprfGroup<CS> as voprf::Group>::ElemLen: Add<<KeGroup<CS> as Group>::PkLen>,
        RegistrationResponseLen<CS>: ArrayLength<u8>,
    {
        <OprfGroup<CS> as voprf::Group>::serialize_elem(self.evaluation_element.value())
            .concat(self.server_s_pk.serialize())
    }

    /// Deserialization from bytes
    pub fn deserialize(mut input: &[u8]) -> Result<Self, ProtocolError> {
        let evaluation_element = EvaluationElement::deserialize(input)?;
        input = &input[EvaluationElementLen::<CS::OprfCs>::USIZE..];

        Ok(Self {
            evaluation_element,
            server_s_pk: PublicKey::deserialize_take(&mut input)?,
        })
    }

    #[cfg(test)]
    /// Only used for tests, where we can set the beta value to test for the
    /// reflection error case
    pub(crate) fn set_evaluation_element_for_testing(
        &self,
        beta: <OprfGroup<CS> as voprf::Group>::Elem,
    ) -> Self {
        Self {
            evaluation_element: voprf::EvaluationElement::from_value_unchecked(beta),
            server_s_pk: self.server_s_pk.clone(),
        }
    }
}

/// Length of [`RegistrationUpload`] in bytes for serialization.
pub type RegistrationUploadLen<CS: CipherSuite> =
    Sum<Sum<<KeGroup<CS> as Group>::PkLen, OutputSize<OprfHash<CS>>>, EnvelopeLen<CS>>;

impl<CS: CipherSuite> RegistrationUpload<CS> {
    /// Serialization into bytes
    pub fn serialize(&self) -> GenericArray<u8, RegistrationUploadLen<CS>>
    where
        // RegistrationUpload: (KePk + Hash) + Envelope
        <KeGroup<CS> as Group>::PkLen: Add<OutputSize<OprfHash<CS>>>,
        Sum<<KeGroup<CS> as Group>::PkLen, OutputSize<OprfHash<CS>>>:
            ArrayLength<u8> + Add<EnvelopeLen<CS>>,
        RegistrationUploadLen<CS>: ArrayLength<u8>,
    {
        self.client_s_pk
            .serialize()
            .concat(self.masking_key.clone())
            .concat(self.envelope.serialize())
    }

    /// Deserialization from bytes
    pub fn deserialize(mut input: &[u8]) -> Result<Self, ProtocolError> {
        Ok(Self {
            client_s_pk: PublicKey::deserialize_take(&mut input)?,
            masking_key: input.take_array("masking key")?,
            envelope: Envelope::deserialize_take(&mut input)?,
        })
    }

    // Creates a dummy instance used for faking a [CredentialResponse]
    pub(crate) fn dummy<R: RngCore + CryptoRng, SK: Clone, OS: Clone>(
        rng: &mut R,
        server_setup: &ServerSetup<CS, SK, OS>,
    ) -> Self {
        let mut masking_key = Output::<OprfHash<CS>>::default();
        rng.fill_bytes(&mut masking_key);

        Self {
            envelope: Envelope::<CS>::dummy(),
            masking_key,
            client_s_pk: server_setup.dummy_pk.clone(),
        }
    }
}

/// Length of [`CredentialRequest`] in bytes for serialization.
pub type CredentialRequestLen<CS: CipherSuite> =
    Sum<<OprfGroup<CS> as voprf::Group>::ElemLen, Ke1MessageLen<CS>>;

impl<CS: CipherSuite> CredentialRequest<CS> {
    /// Serialization into bytes
    pub fn serialize(&self) -> GenericArray<u8, CredentialRequestLen<CS>>
    where
        <CS::KeyExchange as KeyExchange>::KE1Message: Serialize,
        // CredentialRequest: KgPk + Ke1Message
        <OprfGroup<CS> as voprf::Group>::ElemLen: Add<Ke1MessageLen<CS>>,
        CredentialRequestLen<CS>: ArrayLength<u8>,
    {
        <OprfGroup<CS> as voprf::Group>::serialize_elem(self.blinded_element.value())
            .concat(self.ke1_message.serialize())
    }

    /// Deserialization from bytes
    pub fn deserialize(mut input: &[u8]) -> Result<Self, ProtocolError>
    where
        <CS::KeyExchange as KeyExchange>::KE1Message: Deserialize,
    {
        Self::deserialize_take(&mut input)
    }

    pub(crate) fn deserialize_take(input: &mut &[u8]) -> Result<Self, ProtocolError>
    where
        <CS::KeyExchange as KeyExchange>::KE1Message: Deserialize,
    {
        let blinded_element = BlindedElement::deserialize(input)?;
        *input = &input[BlindedElementLen::<CS::OprfCs>::USIZE..];

        Ok(Self {
            blinded_element,
            ke1_message: <CS::KeyExchange as KeyExchange>::KE1Message::deserialize_take(input)?,
        })
    }

    pub(crate) fn to_parts(&self) -> SerializedCredentialRequest<CS> {
        SerializedCredentialRequest::new(&self.blinded_element)
    }

    /// Only used for testing purposes
    #[cfg(test)]
    pub(crate) fn get_blinded_element_for_testing(&self) -> voprf::BlindedElement<CS::OprfCs> {
        self.blinded_element.clone()
    }
}

/// Length of [`CredentialResponse`] in bytes for serialization.
pub type CredentialResponseLen<CS: CipherSuite> =
    Sum<CredentialResponseWithoutKeLen<CS>, Ke2MessageLen<CS>>;

pub(crate) type CredentialResponseWithoutKeLen<CS: CipherSuite> =
    Sum<Sum<<OprfGroup<CS> as voprf::Group>::ElemLen, NonceLen>, MaskedResponseLen<CS>>;

impl<CS: CipherSuite> CredentialResponse<CS> {
    /// Serialization into bytes
    pub fn serialize(&self) -> GenericArray<u8, CredentialResponseLen<CS>>
    where
        <CS::KeyExchange as KeyExchange>::KE2Message: Serialize,
        // CredentialResponseWithoutKeLen: (KgPk + Nonce) + MaskedResponse
        <OprfGroup<CS> as voprf::Group>::ElemLen: Add<NonceLen>,
        Sum<<OprfGroup<CS> as voprf::Group>::ElemLen, NonceLen>:
            ArrayLength<u8> + Add<MaskedResponseLen<CS>>,
        CredentialResponseWithoutKeLen<CS>: ArrayLength<u8>,
        // CredentialResponse: CredentialResponseWithoutKeLen + Ke2Message
        CredentialResponseWithoutKeLen<CS>: Add<Ke2MessageLen<CS>>,
        CredentialResponseLen<CS>: ArrayLength<u8>,
    {
        <OprfGroup<CS> as voprf::Group>::serialize_elem(self.evaluation_element.value())
            .concat(self.masking_nonce)
            .concat(self.masked_response.serialize())
            .concat(self.ke2_message.serialize())
    }

    /// Deserialization from bytes
    pub fn deserialize(mut input: &[u8]) -> Result<Self, ProtocolError>
    where
        <CS::KeyExchange as KeyExchange>::KE2Message: Deserialize,
    {
        let evaluation_element = EvaluationElement::deserialize(input)?;
        input = &input[voprf::EvaluationElementLen::<CS::OprfCs>::USIZE..];

        Ok(Self {
            evaluation_element,
            masking_nonce: input.take_array("masking nonce")?,
            masked_response: MaskedResponse::deserialize_take(&mut input)?,
            ke2_message: <CS::KeyExchange as KeyExchange>::KE2Message::deserialize_take(
                &mut input,
            )?,
        })
    }

    pub(crate) fn to_parts(&self) -> SerializedCredentialResponse<CS> {
        SerializedCredentialResponse::new(
            &self.evaluation_element,
            self.masking_nonce,
            self.masked_response.clone(),
        )
    }

    #[cfg(test)]
    /// Only used for tests, where we can set the beta value to test for the
    /// reflection error case
    pub(crate) fn set_evaluation_element_for_testing(
        &self,
        beta: <OprfGroup<CS> as voprf::Group>::Elem,
    ) -> Self {
        Self {
            evaluation_element: voprf::EvaluationElement::from_value_unchecked(beta),
            masking_nonce: self.masking_nonce,
            masked_response: self.masked_response.clone(),
            ke2_message: self.ke2_message.clone(),
        }
    }
}

/// Length of [`CredentialFinalization`] in bytes for serialization.
pub type CredentialFinalizationLen<CS: CipherSuite> = Ke3MessageLen<CS>;

impl<CS: CipherSuite> CredentialFinalization<CS> {
    /// Serialization into bytes
    pub fn serialize(&self) -> GenericArray<u8, CredentialFinalizationLen<CS>>
    where
        <CS::KeyExchange as KeyExchange>::KE3Message: Serialize,
    {
        self.ke3_message.serialize()
    }

    /// Deserialization from bytes
    pub fn deserialize(mut input: &[u8]) -> Result<Self, ProtocolError>
    where
        <CS::KeyExchange as KeyExchange>::KE3Message: Deserialize,
    {
        Ok(Self {
            ke3_message: <CS::KeyExchange as KeyExchange>::KE3Message::deserialize_take(
                &mut input,
            )?,
        })
    }
}
