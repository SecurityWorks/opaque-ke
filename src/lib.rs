// Copyright (c) Meta Platforms, Inc. and affiliates.
//
// This source code is dual-licensed under either the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree or the Apache
// License, Version 2.0 found in the LICENSE-APACHE file in the root directory
// of this source tree. You may select, at your option, one of the above-listed
// licenses.

//! An implementation of the OPAQUE augmented password authentication key
//! exchange protocol
//!
//! Note: This implementation is in sync with [draft-irtf-cfrg-opaque-16](https://datatracker.ietf.org/doc/draft-irtf-cfrg-opaque/16/),
//! but this specification is subject to change, until the final version
//! published by the IETF.
//!
//! ### Minimum Supported Rust Version
//!
//! Rust **1.85** or higher.
//!
//! # Overview
//!
//! OPAQUE is a protocol between a client and a server. They must first agree on
//! a collection of primitives to be kept consistent throughout protocol
//! execution. These include:
//! * a finite cyclic group along with a point representation
//!   * for the OPRF and
//!   * for the key exchange
//! * a key exchange protocol,
//! * a hashing function, and
//! * a key stretching function.
//!
//! We will use the following choices in this example:
//! ```ignore
//! use opaque_ke::CipherSuite;
//!
//! struct Default;
//!
//! impl CipherSuite for Default {
//!     type OprfCs = opaque_ke::Ristretto255;
//!     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//!     type Ksf = opaque_ke::ksf::Identity;
//! }
//! ```
//! See [examples/simple_login.rs](https://github.com/facebook/opaque-ke/blob/main/examples/simple_login.rs)
//! for a working example of a simple password-based login using OPAQUE.
//!
//! Note that our choice of key stretching function in this example, `Identity`,
//! is selected only to ensure that the tests execute quickly. A real
//! application should use an actual key stretching function, such as `Argon2`,
//! which can be enabled through the `argon2` feature. See more details in
//! the [features](#features) section.
//!
//! ## Setup
//! To set up the protocol, the server begins by creating a `ServerSetup`
//! object:
//! ```
//! # use opaque_ke::errors::ProtocolError;
//! # use opaque_ke::CipherSuite;
//! # use opaque_ke::ServerSetup;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! use rand::RngCore;
//! use rand::rngs::OsRng;
//!
//! let mut rng = OsRng;
//! let server_setup = ServerSetup::<Default>::new(&mut rng);
//! # Ok::<(), ProtocolError>(())
//! ```
//! The server must persist an instance of [`ServerSetup`] for the registration
//! and login steps, and can use [`ServerSetup::serialize`] and
//! [`ServerSetup::deserialize`] to save and restore the instance.
//!
//! ## Registration
//! The registration protocol between the client and server consists of four
//! steps along with three messages: [`RegistrationRequest`],
//! [`RegistrationResponse`], and [`RegistrationUpload`]. A successful execution
//! of the registration protocol results in the server producing a password file
//! corresponding to a server-side identifier for the client, along with the
//! password provided by the client. This password file is typically stored in a
//! key-value database, where the keys consist of these server-side identifiers
//! for each client, and the values consist of their corresponding password
//! files, to be retrieved upon future login attempts made by the client.
//! It is your responsibility to ensure that the identifier used to form the
//! initial [`RegistrationRequest`], typically supplied by the client, matches
//! the database key used in the final [`RegistrationUpload`] step.
//!
//! Note that the [`RegistrationUpload`] message contains sensitive information
//! (about as sensitive as a hash of the password), and hence should be
//! protected with confidentiality guarantees by the consumer of this library.
//!
//! ### Client Registration Start
//! In the first step of registration, the client chooses as input a
//! registration password. The client runs [`ClientRegistration::start`] to
//! produce a [`ClientRegistrationStartResult`], which consists of a
//! [`RegistrationRequest`] to be sent to the server and a
//! [`ClientRegistration`] which must be persisted on the client for the final
//! step of client registration.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ServerRegistration,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! use opaque_ke::ClientRegistration;
//! use rand::RngCore;
//! use rand::rngs::OsRng;
//!
//! let mut client_rng = OsRng;
//! let client_registration_start_result =
//!     ClientRegistration::<Default>::start(&mut client_rng, b"password")?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ### Server Registration Start
//! In the second step of registration, the server takes as input a persisted
//! instance of [`ServerSetup`], a [`RegistrationRequest`] from the client, and
//! a server-side identifier for the client. The server runs
//! [`ServerRegistration::start`] to produce a
//! [`ServerRegistrationStartResult`], which consists of a
//! [`RegistrationResponse`] to be returned to the client.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration,
//! #   ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! use opaque_ke::ServerRegistration;
//!
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! let server_registration_start_result = ServerRegistration::<Default>::start(
//!     &server_setup,
//!     client_registration_start_result.message,
//!     b"alice@example.com",
//! )?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ### Client Registration Finish
//! In the third step of registration, the client takes as input a
//! [`RegistrationResponse`] from the server, and a [`ClientRegistration`] from
//! the first step of registration. The client runs
//! [`ClientRegistration::finish`] to
//! produce a [`ClientRegistrationFinishResult`], which consists of a
//! [`RegistrationUpload`] to be sent to the server and an `export_key` field
//! which can be used optionally as described in the [Export Key](#export-key)
//! section.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ServerRegistration, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! use opaque_ke::ClientRegistrationFinishParameters;
//!
//! let client_registration_finish_result = client_registration_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_registration_start_result.message,
//!     ClientRegistrationFinishParameters::default(),
//! )?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ### Server Registration Finish
//! In the fourth step of registration, the server takes as input a
//! [`RegistrationUpload`] from the client, and a [`ServerRegistration`] from
//! the second step. The server runs [`ServerRegistration::finish`] to produce a
//! finalized [`ServerRegistration`]. At this point, the client can be
//! considered as successfully registered, and the server can invoke
//! [`ServerRegistration::serialize`] to store the password file for use during
//! the login protocol.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::default())?;
//! let password_file = ServerRegistration::<Default>::finish(
//!     client_registration_finish_result.message,
//! );
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ## Login
//! The login protocol between a client and server also consists of four steps
//! along with three messages: [`CredentialRequest`], [`CredentialResponse`],
//! [`CredentialFinalization`]. The server is expected to have access to the
//! password file corresponding to an output of the registration phase (see
//! [Dummy Server Login](#dummy-server-login) for handling the scenario where no
//! password file is available). The login protocol will execute successfully
//! only if the same password was used in the registration phase that produced
//! the password file that the server is testing against.
//!
//! ### Client Login Start
//! In the first step of login, the client chooses as input a login password.
//! The client runs [`ClientLogin::start`] to produce an output consisting of a
//! [`CredentialRequest`] to be sent to the server, and a [`ClientLogin`] which
//! must be persisted on the client for the final step of client login.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ServerRegistration, ServerLogin, CredentialFinalization,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! use opaque_ke::ClientLogin;
//!
//! let mut client_rng = OsRng;
//! let client_login_start_result = ClientLogin::<Default>::start(&mut client_rng, b"password")?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ### Server Login Start
//! In the second step of login, the server takes as input a persisted instance
//! of [`ServerSetup`], the password file output from registration, a
//! [`CredentialRequest`] from the client, and a server-side identifier for the
//! client. The server runs [`ServerLogin::start`] to produce an output
//! consisting of a [`CredentialResponse`] which is returned to the client, and
//! a [`ServerLogin`] which must be persisted on the server for the final step
//! of login.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::default())?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #   &mut client_rng,
//! #   b"password",
//! # )?;
//! use opaque_ke::{ServerLogin, ServerLoginParameters};
//!
//! let password_file = ServerRegistration::<Default>::deserialize(&password_file_bytes)?;
//! let mut server_rng = OsRng;
//! let server_login_start_result = ServerLogin::start(
//!     &mut server_rng,
//!     &server_setup,
//!     Some(password_file),
//!     client_login_start_result.message,
//!     b"alice@example.com",
//!     ServerLoginParameters::default(),
//! )?;
//! # Ok::<(), ProtocolError>(())
//! ```
//! Note that if there is no corresponding password file found for the user, the
//! server can use `None` in place of `Some(password_file)` in order to generate
//! a [`CredentialResponse`] that is indistinguishable from a valid
//! [`CredentialResponse`] returned for a registered client. This allows the
//! server to prevent leaking information about whether or not a client has
//! previously registered with the server.
//!
//! ### Client Login Finish
//! In the third step of login, the client takes as input a
//! [`CredentialResponse`] from the server and runs [`ClientLogin::finish`]
//! on it.
//! If the authentication is successful, then the client obtains a
//! [`ClientLoginFinishResult`]. Otherwise, on failure, the
//! algorithm outputs an
//! [`InvalidLoginError`](errors::ProtocolError::InvalidLoginError) error.
//!
//! The resulting [`ClientLoginFinishResult`] obtained by client in this step
//! contains, among other things, a [`CredentialFinalization`] to be sent to the
//! server to complete the protocol, and a
//! [`session_key`](struct.ClientLoginFinishResult.html#structfield.session_key)
//! which will match the server's session key upon a successful login.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, ServerLogin, ServerLoginParameters, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::default())?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let password_file =
//! #   ServerRegistration::<Default>::deserialize(
//! #     &password_file_bytes,
//! #   )?;
//! # let server_login_start_result =
//! #     ServerLogin::start(&mut server_rng, &server_setup, Some(password_file), client_login_start_result.message, b"alice@example.com", ServerLoginParameters::default())?;
//! use opaque_ke::ClientLoginFinishParameters;
//!
//! let client_login_finish_result = client_login_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_login_start_result.message,
//!     ClientLoginFinishParameters::default(),
//! )?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ### Server Login Finish
//! In the fourth step of login, the server takes as input a
//! [`CredentialFinalization`] from the client and runs [`ServerLogin::finish`]
//! to produce an output consisting of the `session_key` sequence of bytes which
//! will match the client's session key upon a successful login.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, ClientLoginFinishParameters, ServerLogin, ServerLoginParameters, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::default())?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #   &mut client_rng,
//! #   b"password",
//! # )?;
//! # let password_file =
//! #   ServerRegistration::<Default>::deserialize(
//! #     &password_file_bytes,
//! #   )?;
//! # let server_login_start_result =
//! #     ServerLogin::start(&mut server_rng, &server_setup, Some(password_file), client_login_start_result.message, b"alice@example.com", ServerLoginParameters::default())?;
//! # let client_login_finish_result = client_login_start_result.state.finish(
//! #   &mut client_rng,
//! #   b"password",
//! #   server_login_start_result.message,
//! #   ClientLoginFinishParameters::default(),
//! # )?;
//! let server_login_finish_result = server_login_start_result.state.finish(
//!     client_login_finish_result.message,
//!     ServerLoginParameters::default(),
//! )?;
//!
//! assert_eq!(
//!    client_login_finish_result.session_key,
//!    server_login_finish_result.session_key,
//! );
//! # Ok::<(), ProtocolError>(())
//! ```
//! If the protocol completes successfully, then the server obtains a
//! `server_login_finish_result.session_key` which is guaranteed to match
//! `client_login_finish_result.session_key` (see the [Session
//! Key](#session-key) section). Otherwise, on failure, the
//! [`ServerLogin::finish`] algorithm outputs the error
//! [`InvalidLoginError`](errors::ProtocolError::InvalidLoginError).
//!
//! # Advanced Usage
//!
//! This implementation offers support for several optional features of OPAQUE,
//! described below. They are not critical to the execution of the main
//! protocol, but can provide additional security benefits which can be suitable
//! for various applications that rely on OPAQUE for authentication.
//!
//! ## Session Key
//!
//! Upon a successful completion of the OPAQUE protocol (the client runs login
//! with the same password used during registration), the client and server have
//! access to a session key, which is a pseudorandomly distributed byte
//! string (of length equal to the output size of [`voprf::CipherSuite::Hash`])
//! which only the client and server know. Multiple login runs using the
//! same password for the same client will produce different session keys,
//! distributed as uniformly random strings. Thus, the session key can be used
//! to establish a secure channel between the client and server.
//!
//! The session key can be accessed from the `session_key` field of
//! [`ClientLoginFinishResult`] and [`ServerLoginFinishResult`]. See the
//! combination of [Client Login Finish](#client-login-finish) and [Server Login
//! Finish](#server-login-finish) for example usage.
//!
//! ## Checking Server Consistency
//!
//! A [`ClientLoginFinishResult`] contains the `server_s_pk` field, which is
//! represents the static public key of the server that is established during
//! the setup phase. This can be used by the client to verify the authenticity
//! of the server it engages with during the login phase. In particular, the
//! client can check that the static public key of the server supplied during
//! registration (with the `server_s_pk` field of
//! [`ClientRegistrationFinishResult`]) matches this field during login.
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, ClientLoginFinishParameters, ServerLogin, ServerLoginParameters, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! // During registration, the client obtains a ClientRegistrationFinishResult with
//! // a server_s_pk field
//! let client_registration_finish_result = client_registration_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_registration_start_result.message,
//!     ClientRegistrationFinishParameters::default(),
//! )?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let password_file =
//! #   ServerRegistration::<Default>::deserialize(
//! #     &password_file_bytes,
//! #   )?;
//! # let server_login_start_result =
//! #     ServerLogin::start(&mut server_rng, &server_setup, Some(password_file), client_login_start_result.message, b"alice@example.com", ServerLoginParameters::default())?;
//!
//! // And then later, during login...
//! let client_login_finish_result = client_login_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_login_start_result.message,
//!     ClientLoginFinishParameters::default(),
//! )?;
//!
//! // Check that the server's static public key obtained from login matches what
//! // was obtained during registration
//! assert_eq!(
//!     &client_registration_finish_result.server_s_pk,
//!     &client_login_finish_result.server_s_pk,
//! );
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! Note that without this check over the consistency of the server's static
//! public key, a malicious actor could impersonate the registration server if
//! it were able to copy the password file output during registration!
//! Therefore, it is recommended to perform the following check in the
//! application layer if the client can obtain a copy of the server's static
//! public key beforehand.
//!
//!
//! ## Export Key
//!
//! The export key is a pseudorandomly distributed byte string
//! (of length equal to the output size of [`voprf::CipherSuite::Hash`]) output
//! by both the [Client Registration Finish](#client-registration-finish) and
//! [Client Login Finish](#client-login-finish) steps. The same export key
//! string will be output by both functions only if the exact same password is
//! passed to [`ClientRegistration::start`] and [`ClientLogin::start`].
//!
//! The export key retains as much secrecy as the password itself, and is
//! similarly derived through an evaluation of the key stretching function.
//! Hence, only the parties which know the password the client uses during
//! registration and login can recover this secret, as it is never exposed to
//! the server. As a result, the export key can be used (separately from the
//! OPAQUE protocol) to provide confidentiality and integrity to other data
//! which only the client should be able to process. For instance, if the server
//! is expected to maintain any client-side secrets which require a password to
//! access, then this export key can be used to encrypt these secrets so that
//! they remain hidden from the server (see [examples/digital_locker.rs](https://github.com/facebook/opaque-ke/blob/main/examples/digital_locker.rs)
//! for a working example).
//!
//! You can access the export key from the `export_key` field of
//! [`ClientRegistrationFinishResult`] and [`ClientLoginFinishResult`].
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, ClientLoginFinishParameters, ServerLogin, ServerLoginParameters, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! // During registration...
//! let client_registration_finish_result = client_registration_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_registration_start_result.message,
//!     ClientRegistrationFinishParameters::default()
//! )?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let password_file =
//! #   ServerRegistration::<Default>::deserialize(
//! #     &password_file_bytes,
//! #   )?;
//! # let server_login_start_result =
//! #     ServerLogin::start(&mut server_rng, &server_setup, Some(password_file), client_login_start_result.message, b"alice@example.com", ServerLoginParameters::default())?;
//!
//! // And then later, during login...
//! let client_login_finish_result = client_login_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_login_start_result.message,
//!     ClientLoginFinishParameters::default(),
//! )?;
//!
//! assert_eq!(
//!     client_registration_finish_result.export_key,
//!     client_login_finish_result.export_key,
//! );
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! ## Custom Identifiers
//!
//! Typically when applications use OPAQUE to authenticate a client to a server,
//! the client has a registered username which is sent to the server to identify
//! the corresponding password file established during registration. This
//! username may or may not coincide with the server-side identifier; however,
//! this username must be known to both the client and the server (whereas the
//! server-side identifier does not need to be exposed to the client). The
//! server may also have an identifier corresponding to an entity (e.g.
//! Facebook). By default, neither of these public identifiers need to be
//! supplied to the OPAQUE protocol.
//!
//! But, for applications that wish to cryptographically bind these identities
//! to the registered password file as well as the session key output by the
//! login phase, these custom identifiers can be specified through
//! [`ClientRegistrationFinishParameters`] in [Client Registration
//! Finish](#client-registration-finish):
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, Identifiers, ServerRegistration, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! let client_registration_finish_result = client_registration_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_registration_start_result.message,
//!     ClientRegistrationFinishParameters::new(
//!         Identifiers {
//!             client: Some(b"Alice_the_Cryptographer"),
//!             server: Some(b"Facebook"),
//!         },
//!         None,
//!     ),
//! )?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! The same identifiers must also be supplied using [`ServerLoginParameters`]
//! in [Server Login Start](#server-login-start):
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, CredentialFinalization, Identifiers, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::new(Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") }, None))?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #   &mut client_rng,
//! #   b"password",
//! # )?;
//! # use opaque_ke::{ServerLogin, ServerLoginParameters};
//! # let password_file = ServerRegistration::<Default>::deserialize(&password_file_bytes)?;
//! # let mut server_rng = OsRng;
//! let server_login_start_result = ServerLogin::start(
//!     &mut server_rng,
//!     &server_setup,
//!     Some(password_file),
//!     client_login_start_result.message,
//!     b"alice@example.com",
//!     ServerLoginParameters {
//!         context: None,
//!         identifiers: Identifiers {
//!             client: Some(b"Alice_the_Cryptographer"),
//!             server: Some(b"Facebook"),
//!         },
//!     },
//! )?;
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! as well as [`ClientLoginFinishParameters`] in [Client Login
//! Finish](#client-login-finish):
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, ClientLoginFinishParameters, Identifiers, ServerLogin, ServerLoginParameters, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::new(Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") }, None))?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let password_file =
//! #   ServerRegistration::<Default>::deserialize(
//! #     &password_file_bytes,
//! #   )?;
//! # let server_login_start_result =
//! #     ServerLogin::start(&mut server_rng, &server_setup, Some(password_file), client_login_start_result.message, b"alice@example.com", ServerLoginParameters { context: None, identifiers: Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") } })?;
//! let client_login_finish_result = client_login_start_result.state.finish(
//!     &mut client_rng,
//!     b"password",
//!     server_login_start_result.message,
//!     ClientLoginFinishParameters::new(
//!         None,
//!         Identifiers {
//!             client: Some(b"Alice_the_Cryptographer"),
//!             server: Some(b"Facebook"),
//!         },
//!         None,
//!     ),
//! )?;
//!
//! # Ok::<(), ProtocolError>(())
//! ```
//! and in [`ServerLoginParameters`] in [Server Login
//! Finish](#server-login-finish):
//! ```
//! # use opaque_ke::{
//! #   errors::ProtocolError,
//! #   ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, ClientLogin, ClientLoginFinishParameters, Identifiers, ServerLogin, ServerLoginParameters, CredentialFinalization, ServerSetup,
//! #   ksf::Identity,
//! # };
//! # use opaque_ke::CipherSuite;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # use rand::{rngs::OsRng, RngCore};
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let mut server_rng = OsRng;
//! # let server_setup = ServerSetup::<Default>::new(&mut server_rng);
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut client_rng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::new(Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") }, None))?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #     &mut client_rng,
//! #     b"password",
//! # )?;
//! # let password_file =
//! #   ServerRegistration::<Default>::deserialize(
//! #     &password_file_bytes,
//! #   )?;
//! # let server_login_start_result =
//! #     ServerLogin::start(&mut server_rng, &server_setup, Some(password_file), client_login_start_result.message, b"alice@example.com", ServerLoginParameters { context: None, identifiers: Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") } })?;
//! # let client_login_finish_result = client_login_start_result.state.finish(
//! #   &mut client_rng,
//! #   b"password",
//! #   server_login_start_result.message,
//! #   ClientLoginFinishParameters::new(None, Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") }, None),
//! # )?;
//! let server_login_finish_result = server_login_start_result.state.finish(
//!     client_login_finish_result.message,
//!     ServerLoginParameters { context: None, identifiers: Identifiers { client: Some(b"Alice_the_Cryptographer"), server: Some(b"Facebook") } },
//! )?;
//!
//! # Ok::<(), ProtocolError>(())
//! ```
//! Failing to supply the same pair of custom identifiers in any of the three
//! steps above will result in an error in attempting to complete the protocol!
//!
//! Note that if only one of the client and server identifiers are present, then
//! [Identifiers] can be used to specify them individually.
//!
//! ## Key Exchange Context
//!
//! A key exchange protocol typically allows for the specifying of shared
//! "context" information between the two parties before the exchange is
//! complete, so as to bind the integrity of application-specific data or
//! configuration parameters to the security of the key exchange. During the
//! login phase, the client and server can specify this context using:
//! - In [Server Login Start](#server-login-start), where the server can
//!   populate [`ServerLoginParameters::context`].
//! - In [Client Login Finish](#client-login-finish), where the client can
//!   populate [`ClientLoginFinishParameters::context`].
//! - In [Server Login Finish](#server-login-finish), where the server can
//!   populate [`ServerLoginParameters::context`].
//!
//! ## Dummy Server Login
//!
//! For applications in which the server does not wish to reveal to the client
//! whether an existing password file has been registered, the server can return
//! a "dummy" credential response message to the client for an unregistered
//! client, which is indistinguishable from the normal credential response
//! message that the server would return for a registered client. The dummy
//! message is created by passing a `None` to the `password_file` parameter for
//! [`ServerLogin::start`].
//!
//! ## Remote Private Keys
//!
//! Servers that want to store their private key in an external location (e.g.
//! in an HSM or vault) can do so with [`ServerLogin::builder()`] without
//! exposing the bytes of the private key to this library.
//! ```
//! # use generic_array::{GenericArray, typenum::U0};
//! # use opaque_ke::{CipherSuite, ClientLogin, ClientRegistration, ClientRegistrationFinishParameters, ServerRegistration, keypair::{PrivateKey, PublicKey}, key_exchange::{KeyExchange, group::Group, tripledh::DiffieHellman}};
//! # use rand::rngs::OsRng;
//! # type Ristretto255 = <<Default as CipherSuite>::KeyExchange as KeyExchange>::Group;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[derive(Debug, thiserror::Error)]
//! # #[error("test error")]
//! # struct YourRemoteKeyError;
//! # #[derive(Clone)]
//! # struct YourRemoteKey(<Ristretto255 as Group>::Sk);
//! # impl YourRemoteKey {
//! #     fn diffie_hellman(&self, pk: &PublicKey<Ristretto255>) -> Result<GenericArray<u8, <Ristretto255 as Group>::PkLen>, YourRemoteKeyError> {
//! #         Ok(<<Ristretto255 as Group>::Sk as DiffieHellman<Ristretto255>>::diffie_hellman(self.0, pk.to_group_type()))
//! #     }
//! # }
//! use opaque_ke::{ServerLogin, ServerLoginParameters, ServerSetup};
//! use opaque_ke::keypair::{KeyPair, PrivateKeySerialization};
//! use opaque_ke::errors::ProtocolError;
//!
//! // Implement if you intend to use `ServerSetup::de/serialize` instead of `serde`.
//! impl PrivateKeySerialization<Ristretto255> for YourRemoteKey {
//!     type Error = YourRemoteKeyError;
//!     type Len = U0;
//!
//!     fn serialize_key_pair(_: &KeyPair<Ristretto255, Self>) -> GenericArray<u8, Self::Len> {
//!         unimplemented!()
//!     }
//!
//!     fn deserialize_take_key_pair(input: &mut &[u8]) -> Result<KeyPair<Ristretto255, Self>, ProtocolError<Self::Error>> {
//!         unimplemented!()
//!     }
//! }
//!
//! # let sk = Ristretto255::random_sk(&mut OsRng);
//! # let pk = Ristretto255::public_key(sk);
//! # let pk = Ristretto255::serialize_pk(pk);
//! # let public_key = PublicKey::deserialize(&pk).unwrap();
//! # let remote_key = YourRemoteKey(sk);
//! # let mut server_rng = OsRng;
//! let keypair = KeyPair::new(remote_key, public_key);
//! let server_setup = ServerSetup::<Default, YourRemoteKey>::new_with_key_pair(&mut server_rng, keypair);
//!
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut OsRng,
//! #     b"password",
//! # )?;
//! # let server_registration_start_result = ServerRegistration::<Default>::start(&server_setup, client_registration_start_result.message, b"alice@example.com")?;
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut OsRng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::default())?;
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #   &mut OsRng,
//! #   b"password",
//! # )?;
//! # let password_file = ServerRegistration::<Default>::deserialize(&password_file_bytes)?;
//! // Use `ServerLogin::builder()` instead of `ServerLogin::start()`.
//! let server_login_builder = ServerLogin::builder(
//!     &mut server_rng,
//!     &server_setup,
//!     Some(password_file),
//!     client_login_start_result.message,
//!     b"alice@example.com",
//!     ServerLoginParameters::default(),
//! )?;
//!
//! // Run Diffie-Hellman on your remote key.
//! let client_e_public_key = server_login_builder.data();
//! let shared_secret = server_login_builder.private_key().diffie_hellman(&client_e_public_key)?;
//!
//! // Use the shared secret to build `ServerLogin`.
//! let server_login_start_result = server_login_builder.build(shared_secret)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Remote OPRF Seeds
//!
//! In addition, the OPRF seed can be stored in an external location as well, by
//! using [`ServerRegistration::start_with_key_material()`] and
//! [`ServerLogin::builder_with_key_material()`] in combination with
//! [`ServerSetup::key_material_info()`].
//! ```
//! # use digest::Output;
//! # use generic_array::{GenericArray, typenum::U0};
//! # use hkdf::Hkdf;
//! # use opaque_ke::{CipherSuite, ClientLogin, ClientRegistration, ClientRegistrationFinishParameters, keypair::{PrivateKey, PublicKey}, key_exchange::{KeyExchange, group::Group, tripledh::DiffieHellman}};
//! # use rand::rngs::OsRng;
//! # use rand::RngCore;
//! # type Ristretto255 = <<Default as CipherSuite>::KeyExchange as KeyExchange>::Group;
//! # type Hash = <<Default as CipherSuite>::KeyExchange as KeyExchange>::Hash;
//! # type OprfGroup = <<Default as CipherSuite>::OprfCs as voprf::CipherSuite>::Group;
//! # struct Default;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for Default {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for Default {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = opaque_ke::ksf::Identity;
//! # }
//! # #[derive(Debug, thiserror::Error)]
//! # #[error("test error")]
//! # struct YourRemoteSecretsError;
//! # #[derive(Clone)]
//! # struct YourRemoteSeed(Output<Hash>);
//! # impl YourRemoteSeed {
//! #     fn hkdf(&self, info: &[&[u8]]) -> GenericArray<u8, <OprfGroup as voprf::Group>::ScalarLen> {
//! #         let mut ikm = GenericArray::default();
//! #         Hkdf::<Hash>::from_prk(&self.0)
//! #             .unwrap()
//! #             .expand_multi_info(info, &mut ikm)
//! #             .unwrap();
//! #         ikm
//! #     }
//! # }
//! # #[derive(Clone)]
//! # struct YourRemoteKey(<Ristretto255 as Group>::Sk);
//! # impl YourRemoteKey {
//! #     fn diffie_hellman(&self, pk: &PublicKey<Ristretto255>) -> Result<GenericArray<u8, <Ristretto255 as Group>::PkLen>, YourRemoteSecretsError> {
//! #         Ok(<<Ristretto255 as Group>::Sk as DiffieHellman<Ristretto255>>::diffie_hellman(self.0, pk.to_group_type()))
//! #     }
//! # }
//! use opaque_ke::{ServerLogin, ServerLoginParameters, ServerRegistration, ServerSetup};
//! use opaque_ke::keypair::{KeyPair, OprfSeedSerialization};
//! use opaque_ke::errors::ProtocolError;
//!
//! // Implement if you intend to use `ServerSetup::de/serialize` instead of `serde`.
//! impl OprfSeedSerialization<sha2::Sha512, YourRemoteSecretsError> for YourRemoteSeed {
//!     type Len = U0;
//!
//!     fn serialize(&self) -> GenericArray<u8, Self::Len> {
//!         unimplemented!()
//!     }
//!
//!     fn deserialize_take(input: &mut &[u8]) -> Result<YourRemoteSeed, ProtocolError<YourRemoteSecretsError>> {
//!         unimplemented!()
//!     }
//! }
//!
//! # let mut oprf_seed = YourRemoteSeed(GenericArray::default());
//! # OsRng.fill_bytes(&mut oprf_seed.0);
//! # let sk = Ristretto255::random_sk(&mut OsRng);
//! # let pk = Ristretto255::public_key(sk);
//! # let pk = Ristretto255::serialize_pk(pk);
//! # let public_key = PublicKey::deserialize(&pk).unwrap();
//! # let remote_key = YourRemoteKey(sk);
//! # let mut server_rng = OsRng;
//! let keypair = KeyPair::new(remote_key, public_key);
//! let server_setup = ServerSetup::<Default, YourRemoteKey, YourRemoteSeed>::new_with_key_pair_and_seed(&mut server_rng, keypair, oprf_seed);
//!
//! // Incoming registration ...
//! # let client_registration_start_result = ClientRegistration::<Default>::start(
//! #     &mut OsRng,
//! #     b"password",
//! # )?;
//!
//! // Run HKDF on your remote OPRF seed.
//! let info = server_setup.key_material_info(b"alice@example.com");
//! let key_material = info.ikm.hkdf(&info.info);
//!
//! // Use `ServerRegistration::start_with_key_material()` instead of `ServerRegistration::start()`.
//! let server_registration_start_result = ServerRegistration::<Default>::start_with_key_material(
//!     &server_setup,
//!     key_material,
//!     client_registration_start_result.message,
//! )?;
//!
//! // Finish registration ...
//! # let client_registration_finish_result = client_registration_start_result.state.finish(&mut OsRng, b"password", server_registration_start_result.message, ClientRegistrationFinishParameters::default())?;
//!
//! // Incoming login ...
//! # let password_file_bytes = ServerRegistration::<Default>::finish(client_registration_finish_result.message).serialize();
//! # let client_login_start_result = ClientLogin::<Default>::start(
//! #   &mut OsRng,
//! #   b"password",
//! # )?;
//! # let password_file = ServerRegistration::<Default>::deserialize(&password_file_bytes)?;
//!
//! // Run HKDF on your remote OPRF seed.
//! let info = server_setup.key_material_info(b"alice@example.com");
//! let key_material = info.ikm.hkdf(&info.info);
//!
//! // Use `ServerLogin::builder_with_key_material()` instead of `ServerLogin::start()`.
//! let server_login_builder = ServerLogin::builder_with_key_material(
//!     &mut server_rng,
//!     &server_setup,
//!     key_material,
//!     Some(password_file),
//!     client_login_start_result.message,
//!     ServerLoginParameters::default(),
//! )?;
//!
//! // Run Diffie-Hellman on your remote key.
//! let client_e_public_key = server_login_builder.data();
//! let shared_secret = server_login_builder.private_key().diffie_hellman(&client_e_public_key)?;
//!
//! // Use the shared secret to build `ServerLogin`.
//! let server_login_start_result = server_login_builder.build(shared_secret)?;
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Custom KSF and Parameters
//!
//! An application might want to use a custom KSF (Key Stretching Function)
//! that's not supported directly by this crate. The maintainer of the said KSF
//! or of the application itself can implement the [`Ksf`](ksf::Ksf) trait to
//! use it with `opaque-ke`. `scrypt` is used for this example, but any KSF
//! can be used.
//! ```
//! # use generic_array::GenericArray;
//! use opaque_ke::ksf::Ksf;
//!
//! #[derive(Default)]
//! struct CustomKsf(scrypt::Params);
//!
//! // The Ksf trait must be implemented to be used in the ciphersuite.
//! impl Ksf for CustomKsf {
//!     fn hash<L: generic_array::ArrayLength<u8>>(
//!         &self,
//!         input: GenericArray<u8, L>,
//!     ) -> Result<GenericArray<u8, L>, opaque_ke::errors::InternalError> {
//!         let mut output = GenericArray::<u8, L>::default();
//!         scrypt::scrypt(&input, &[], &self.0, &mut output)
//!             .map_err(|_| opaque_ke::errors::InternalError::KsfError)?;
//!
//!         Ok(output)
//!     }
//! }
//! ```
//!
//! It is also possible to override the default derivation parameters that are
//! used by the KSF during registration and login. This can be especially
//! helpful if the `Ksf` trait is already implemented.
//! ```
//! # use opaque_ke::CipherSuite;
//! # use opaque_ke::ClientRegistration;
//! # use opaque_ke::ClientRegistrationFinishParameters;
//! # use opaque_ke::ServerSetup;
//! # use opaque_ke::errors::ProtocolError;
//! # use rand::rngs::OsRng;
//! # use rand::RngCore;
//! # use std::default::Default;
//! # #[cfg(feature = "argon2")]
//! # {
//! # struct DefaultCipherSuite;
//! # #[cfg(feature = "ristretto255")]
//! # impl CipherSuite for DefaultCipherSuite {
//! #     type OprfCs = opaque_ke::Ristretto255;
//! #     type KeyExchange = opaque_ke::TripleDh<opaque_ke::Ristretto255, sha2::Sha512>;
//! #     type Ksf = argon2::Argon2<'static>;
//! # }
//! # #[cfg(not(feature = "ristretto255"))]
//! # impl CipherSuite for DefaultCipherSuite {
//! #     type OprfCs = p256::NistP256;
//! #     type KeyExchange = opaque_ke::TripleDh<p256::NistP256, sha2::Sha256>;
//! #     type Ksf = argon2::Argon2<'static>;
//! # }
//! #
//! # let password = b"password";
//! # let mut rng = OsRng;
//! # let server_setup = ServerSetup::<DefaultCipherSuite>::new(&mut rng);
//! # let mut client_rng = OsRng;
//! # let client_registration_start_result =
//! #     ClientRegistration::<DefaultCipherSuite>::start(&mut client_rng, password)?;
//! # use opaque_ke::ServerRegistration;
//! # let server_registration_start_result = ServerRegistration::<DefaultCipherSuite>::start(
//! #     &server_setup,
//! #     client_registration_start_result.message,
//! #     b"alice@example.com",
//! # )?;
//! #
//! // Create an Argon2 instance with the specified parameters
//! let argon2_params = argon2::Params::new(131072, 2, 4, None).unwrap();
//! let argon2_params = argon2::Argon2::new(
//!     argon2::Algorithm::Argon2id,
//!     argon2::Version::V0x13,
//!     argon2_params,
//! );
//!
//! // Override the default parameters with the custom ones
//! let hash_params = ClientRegistrationFinishParameters {
//!     ksf: Some(&argon2_params),
//!     ..Default::default()
//! };
//!
//! let client_registration_finish_result = client_registration_start_result
//!     .state
//!     .finish(
//!         &mut rng,
//!         password,
//!         server_registration_start_result.message,
//!         hash_params,
//!     )
//!     .unwrap();
//! # }
//! # Ok::<(), ProtocolError>(())
//! ```
//!
//! # Features
//!
//! - The `argon2` feature, when enabled, introduces a dependency on `argon2`
//!   and implements the `Ksf` trait for `Argon2` with a set of default parameters.
//!   In general, secure instantiations should choose to invoke a memory-hard password
//!   hashing function when the client's password is expected to have low entropy,
//!   instead of relying on [`ksf::Identity`] as done in the above example. The
//!   more computationally intensive the `Ksf` function is, the more resistant
//!   the server's password file records will be against offline dictionary and precomputation
//!   attacks; see [the OPAQUE paper](https://eprint.iacr.org/2018/163.pdf) for
//!   more details. The `argon2` feature requires [`alloc`].
//!
//! - The `serde` feature, enabled by default, provides convenience functions for serializing and deserializing with [serde](https://serde.rs/).
//!
//! - The `ristretto255` feature enables using [`Ristretto255`] as a `KeGroup`
//!   and `OprfCs`. To select a specific backend see the [curve25519-dalek]
//!   documentation.
//!
//! - The `curve25519` feature enables Curve25519 as a `KeGroup`. To select a
//!   specific backend see the [curve25519-dalek] documentation.
//!
//! - The `ecdsa` feature enables using [`elliptic_curve`]s with [`Ecdsa`] for
//!   [`SigmaI`]s signature algorithm.
//!
//! - The `ed25519` feature enables using [`Ed25519`]s with [`PureEddsa`] and
//!   [`HashEddsa`] for [`SigmaI`]s signature algorithm.
//!
//! [`alloc`]: https://doc.rust-lang.org/alloc
//! [curve25519-dalek]: https://docs.rs/curve25519-dalek/4/curve25519_dalek/index.html#backends

#![no_std]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(not(test), deny(unsafe_code))]
#![warn(clippy::cargo, clippy::doc_markdown, missing_docs, rustdoc::all)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![allow(type_alias_bounds)]

#[cfg(any(feature = "std", test))]
extern crate std;

// Error types
pub mod errors;

pub mod ciphersuite;
mod envelope;
pub mod hash;
pub mod key_exchange;
pub mod keypair;
pub mod ksf;
mod messages;
mod opaque;
mod serialization;

#[cfg(test)]
mod tests;

// Exports

pub use rand;

pub use crate::ciphersuite::CipherSuite;
#[cfg(feature = "curve25519")]
pub use crate::key_exchange::group::curve25519::Curve25519;
#[cfg(feature = "ed25519")]
pub use crate::key_exchange::group::ed25519::Ed25519;
#[cfg(feature = "ristretto255")]
pub use crate::key_exchange::group::ristretto255::Ristretto255;
pub use crate::key_exchange::sigma_i::SigmaI;
#[cfg(feature = "ecdsa")]
pub use crate::key_exchange::sigma_i::ecdsa::Ecdsa;
pub use crate::key_exchange::sigma_i::hash_eddsa::HashEddsa;
pub use crate::key_exchange::sigma_i::pure_eddsa::PureEddsa;
pub use crate::key_exchange::tripledh::TripleDh;
pub use crate::messages::{
    CredentialFinalization, CredentialFinalizationLen, CredentialRequest, CredentialRequestLen,
    CredentialResponse, CredentialResponseLen, RegistrationRequest, RegistrationRequestLen,
    RegistrationResponse, RegistrationResponseLen, RegistrationUpload, RegistrationUploadLen,
    ServerLoginBuilder,
};
pub use crate::opaque::{
    ClientLogin, ClientLoginFinishParameters, ClientLoginFinishResult, ClientLoginStartResult,
    ClientRegistration, ClientRegistrationFinishParameters, ClientRegistrationFinishResult,
    ClientRegistrationStartResult, Identifiers, KeyMaterialInfo, ServerLogin,
    ServerLoginFinishResult, ServerLoginParameters, ServerLoginStartResult, ServerRegistration,
    ServerRegistrationLen, ServerRegistrationStartResult, ServerSetup,
};
