// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `BN*` library types and members the native backend recognizes:
//! whether a type is (or carries) a `BNJson`, `BNCrypto`, `BNCompress` or
//! `BNSqlite` handle, and which member of a library a call selects.
#![allow(clippy::wildcard_imports)]
use super::provider_name;
use crate::*;

/// True when `ty` carries a `BNJson.Json`, directly or inside an `OR Error`
/// alternative (a narrowed value keeps the aggregate, as `FS.File` does).
#[cfg(feature = "json")]
pub(crate) fn carries_bnjson(module: &Module, ty: &Type) -> bool {
    let named = |ty: &Type| {
        matches!(
            ty,
            Type::ImportedNamed { module: module_id, name }
                if name == "Json"
                    && module
                        .bnjson_providers
                        .contains(&bn_ir::ModuleId::from(*module_id))
        )
    };
    match ty {
        Type::Alternative(alternatives) => alternatives.iter().any(named),
        other => named(other),
    }
}

/// The `BNJson` member `name` selects, if this module imports `BNJson`.
#[cfg(feature = "json")]
pub(crate) fn bnjson_member<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    let method = name.rsplit('.').next()?;
    (!module.bnjson_providers.is_empty()
        && matches!(
            method,
            "Parse"
                | "Stringify"
                | "Object"
                | "Array"
                | "Kind"
                | "Has"
                | "Length"
                | "Clone"
                | "SetString"
                | "SetInteger"
                | "SetFloat"
                | "SetBoolean"
                | "SetNull"
                | "SetJson"
                | "GetString"
                | "GetInteger"
                | "GetFloat"
                | "GetBoolean"
                | "GetJson"
                | "AppendString"
                | "AppendInteger"
                | "AppendFloat"
                | "AppendBoolean"
                | "AppendNull"
                | "AppendJson"
                | "GetStringAt"
                | "GetIntegerAt"
                | "GetFloatAt"
                | "GetBooleanAt"
                | "GetJsonAt"
                | "SetStringAt"
                | "SetIntegerAt"
                | "SetFloatAt"
                | "SetBooleanAt"
                | "SetNullAt"
                | "SetJsonAt"
        ))
    .then_some(method)
}

/// True when `ty` is `BNCrypto.Bytes` from a module this program imports.
pub(crate) fn is_bncrypto_bytes_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "Bytes"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.Bytes`, either directly or inside an
/// `OR Error` alternative. A narrowed value keeps the aggregate type, exactly as
/// `FS.File` does, so both shapes reach method calls.
pub(crate) fn carries_bncrypto_bytes(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_bytes_type(module, alternative)),
        other => is_bncrypto_bytes_type(module, other),
    }
}

/// True when `ty` is `BNCrypto.Key` from a module this program imports.
pub(crate) fn is_bncrypto_key_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "Key"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.Key`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncrypto_key(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_key_type(module, alternative)),
        other => is_bncrypto_key_type(module, other),
    }
}

/// True when `ty` is `BNCrypto.Secret` from a module this program imports.
pub(crate) fn is_bncrypto_secret_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "Secret"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.Secret`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncrypto_secret(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_secret_type(module, alternative)),
        other => is_bncrypto_secret_type(module, other),
    }
}

/// True when `ty` is `BNCrypto.SecretKey` from a module this program imports.
pub(crate) fn is_bncrypto_secret_key_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "SecretKey"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.SecretKey`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncrypto_secret_key(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_secret_key_type(module, alternative)),
        other => is_bncrypto_secret_key_type(module, other),
    }
}

/// True when `ty` is `BNCrypto.PublicKey` from a module this program imports.
pub(crate) fn is_bncrypto_public_key_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name,
        } if name == "PublicKey"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.PublicKey`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncrypto_public_key(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_public_key_type(module, alternative)),
        other => is_bncrypto_public_key_type(module, other),
    }
}

/// The `BNCrypto` handle types whose `RELEASE` ends their runtime table entry:
/// type name, register slot prefix, release symbol. A `KeyPair` is a STRUCT
/// and is not here: releasing it leaves the keys it holds alive (as `bni`).
const BNCRYPTO_HANDLE_RELEASES: [(&str, &str, &str); 4] = [
    ("Hasher", "cryhs", "bn_rt_crypto_hasher_release"),
    ("MacStream", "crymac", "bn_rt_crypto_mac_stream_release"),
    ("PublicKey", "crypub", "bn_rt_crypto_public_key_release"),
    ("PrivateKey", "crypriv", "bn_rt_crypto_private_key_release"),
];

/// The slot prefix and release symbol of a `BNCrypto` handle type from a
/// module this program imports, or `None`.
pub(crate) fn bncrypto_handle_release(
    module: &Module,
    ty: &Type,
) -> Option<(&'static str, &'static str)> {
    let Type::ImportedNamed {
        module: module_id,
        name,
    } = ty
    else {
        return None;
    };
    if !module
        .bncrypto_providers
        .contains(&bn_ir::ModuleId::from(*module_id))
    {
        return None;
    }
    BNCRYPTO_HANDLE_RELEASES
        .iter()
        .find(|(type_name, _, _)| *type_name == name.as_str())
        .map(|(_, slot, symbol)| (*slot, *symbol))
}

/// The release symbol of a `BNCrypto` handle value given its exact type:
/// `Bytes`, `Key`, `Secret`, `SecretKey` and the handle table above.
fn bncrypto_value_release(module: &Module, ty: &Type) -> Option<(&'static str, &'static str)> {
    if is_bncrypto_bytes_type(module, ty) {
        Some(("cry", "bn_rt_crypto_bytes_release"))
    } else if is_bncrypto_key_type(module, ty) {
        Some(("cryk", "bn_rt_crypto_key_release"))
    } else if is_bncrypto_secret_type(module, ty) {
        Some(("crys", "bn_rt_crypto_secret_release"))
    } else if is_bncrypto_secret_key_type(module, ty) {
        Some(("crysk", "bn_rt_crypto_secret_key_release"))
    } else {
        bncrypto_handle_release(module, ty)
    }
}

/// What `RELEASE` does with a `BNCrypto` value inside an `OR Error`
/// alternative (the `{ i1, ptr, i64 }` status result, handle in slot 2 when
/// the tag says it is not an error).
pub(crate) enum CryptoAlternativeRelease {
    /// Release the handle in slot 2 with this slot prefix and symbol.
    Handle(&'static str, &'static str),
    /// A `KeyPair` record: no runtime work (the keys in it stay alive, as in
    /// `bni`; the record itself is not freed: native code does not yet release
    /// library storage at the end of a scope).
    Record,
}

/// The `RELEASE` treatment of an alternative that carries a `BNCrypto` value,
/// or `None` when it carries none.
pub(crate) fn bncrypto_alternative_release(
    module: &Module,
    ty: &Type,
) -> Option<CryptoAlternativeRelease> {
    let Type::Alternative(alternatives) = ty else {
        return None;
    };
    alternatives.iter().find_map(|alternative| {
        if let Some((slot, symbol)) = bncrypto_value_release(module, alternative) {
            return Some(CryptoAlternativeRelease::Handle(slot, symbol));
        }
        matches!(
            alternative,
            Type::ImportedNamed { module: module_id, name } if name == "KeyPair"
                && module.bncrypto_providers.contains(&bn_ir::ModuleId::from(*module_id))
        )
        .then_some(CryptoAlternativeRelease::Record)
    })
}

/// True when `ty` is `BNCrypto.PrivateKey` from a module this program imports.
pub(crate) fn is_bncrypto_private_key_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name,
        } if name == "PrivateKey"
            && module
                .bncrypto_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCrypto.PrivateKey`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncrypto_private_key(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncrypto_private_key_type(module, alternative)),
        other => is_bncrypto_private_key_type(module, other),
    }
}

/// The `BNCrypto` member `name` selects, if this module imports `BNCrypto` and
/// the callee is part of the supported surface.
/// The `BNCrypto` members that build a `KeyPair` record on the heap (`calloc`):
/// the analysis marks a function that calls one as a heap user, so the preamble
/// declares the allocator even when nothing else in the program needs it.
pub(crate) const KEYPAIR_CONSTRUCTORS: [&str; 3] =
    ["NewKeyPair", "NewKeyPairFor", "NewKeyPairWith"];

pub(crate) fn bncrypto_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    if name.starts_with('#')
        && provider_name(module, name) != Some(bn_catalog::text("modules", "module.crypto"))
    {
        return None;
    }
    let method = name.rsplit('.').next()?;
    (!module.bncrypto_providers.is_empty()
        && matches!(
            method,
            "SHA256"
                | "SHA512"
                | "FromText"
                | "FromHex"
                | "Length"
                | "ToHex"
                | "SealAesGcm"
                | "OpenAesGcm"
                | "SealChaCha20"
                | "OpenChaCha20"
                | "HmacSha256"
                | "VerifyHmacSha256"
                | "Argon2id"
                | "Ed25519PublicKey"
                | "Ed25519Sign"
                | "Ed25519Verify"
                | "EcdsaP256PublicKey"
                | "EcdsaP256Sign"
                | "EcdsaP256Verify"
                | "Slice"
                | "MlKemKeypair"
                | "MlKemEncapsulate"
                | "MlKemDecapsulate"
                | "MlDsaKeypair"
                | "MlDsaSign"
                | "MlDsaVerify"
                | "RandomBytes"
                | "GenerateNonce"
                | "GenerateKey"
                | "ImportKey"
                | "Algorithm"
                | "Zeroize"
                | "ConstantTimeEquals"
                | "NewSecret"
                | "SecretFromText"
                | "ToBytes"
                | "RandomInt"
                | "RandomToken"
                | "RandomUuidV4"
                | "UuidV7"
                | "RandomPassword"
                | "NewKeyPair"
                | "NewKeyPairFor"
                | "NewKeyPairWith"
                | "PublicKey"
                | "Sign"
                | "SignFile"
                | "VerifyFile"
                | "ExportPem"
                | "ExportPemUnencrypted"
                | "Fingerprint"
                | "LoadPrivateKey"
                | "LoadPublicKey"
                | "NewSecretKey"
                | "NewSecretKeyWith"
                | "SecretKeyFromBytes"
                | "WrapKey"
                | "UnwrapKey"
                | "UnwrapKeyWith"
                | "Id"
                | "Bits"
                | "Purpose"
                | "ExportSecret"
                | "Encrypt"
                | "EncryptWith"
                | "HashPasswordWith"
                | "NeedsRehash"
                | "DeriveKey"
                | "DeriveSubkey"
                | "DecryptWith"
                | "Decrypt"
                | "EncryptText"
                | "DecryptText"
                | "EncryptFile"
                | "DecryptFile"
                | "Hash"
                | "HashText"
                | "HashFile"
                | "HashWith"
                | "Mac"
                | "VerifyMac"
                | "MacWith"
                | "HashPassword"
                | "VerifyPassword"
                | "Hkdf"
                | "New"
                | "NewWith"
                | "Update"
                | "Finish"
                | "Verify"
                | "ToText"
                | "ToBase64"
                | "FromBase64"
                | "ToBase64Url"
                | "FromBase64Url"
                | "Concat"
                | "ToPem"
                | "FromPem"
                | "Crc32"
                | "Crc32c"
                | "Adler32"
                | "XxHash64"
                | "VerifyFileChecksum"
        ))
    .then_some(method)
}

/// True when `ty` is `BNCompress.Bytes` from a module this program imports.
pub(crate) fn is_bncompress_bytes_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "Bytes"
            && module
                .bncompress_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCompress.Bytes`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncompress_bytes(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncompress_bytes_type(module, alternative)),
        other => is_bncompress_bytes_type(module, other),
    }
}

/// True when `ty` is `BNCompress.ZipArchive` from a module this program imports.
pub(crate) fn is_bncompress_zip_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name
        } if name == "ZipArchive"
            && module
                .bncompress_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNCompress.ZipArchive`, either directly or inside an
/// `OR Error` alternative.
pub(crate) fn carries_bncompress_zip(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bncompress_zip_type(module, alternative)),
        other => is_bncompress_zip_type(module, other),
    }
}

/// The `BNCompress` member `name` selects, if this module imports `BNCompress` and
/// the callee is part of the supported surface.
pub(crate) fn bncompress_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    if name.starts_with('#')
        && provider_name(module, name) != Some(bn_catalog::text("modules", "module.compress"))
    {
        return None;
    }
    let method = name.rsplit('.').next()?;
    (!module.bncompress_providers.is_empty()
        && matches!(
            method,
            "Deflate"
                | "Inflate"
                | "Gzip"
                | "Gunzip"
                | "DeflateText"
                | "InflateText"
                | "GzipText"
                | "GunzipText"
                | "FromText"
                | "FromHex"
                | "Length"
                | "ToHex"
                | "ToText"
                | "Create"
                | "Open"
                | "AddFile"
                | "AddText"
                | "AddStoredText"
                | "AddBytes"
                | "AddStoredBytes"
                | "ExtractAll"
                | "ExtractFile"
                | "ExtractText"
                | "ExtractBytes"
                | "EntryCount"
                | "EntryName"
                | "Close"
        ))
    .then_some(method)
}

/// True when `ty` is `BNSqlite.Connection` from a module this program imports.
#[cfg(feature = "sqlite")]
pub(crate) fn is_bnsqlite_connection_type(module: &Module, ty: &Type) -> bool {
    matches!(
        ty,
        Type::ImportedNamed {
            module: module_id,
            name,
        } if name == "Connection"
            && module
                .bnsqlite_providers
                .contains(&bn_ir::ModuleId::from(*module_id))
    )
}

/// True when `ty` carries a `BNSqlite.Connection`, either directly or inside an
/// `OR Error` alternative.
#[cfg(feature = "sqlite")]
pub(crate) fn carries_bnsqlite_connection(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::Alternative(alternatives) => alternatives
            .iter()
            .any(|alternative| is_bnsqlite_connection_type(module, alternative)),
        other => is_bnsqlite_connection_type(module, other),
    }
}

/// The `BNSqlite` member `name` selects, if this module imports `BNSqlite` and
/// the callee is part of the supported surface.
#[cfg(feature = "sqlite")]
pub(crate) fn bnsqlite_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    let method = name.rsplit('.').next()?;
    (!module.bnsqlite_providers.is_empty()
        && matches!(
            method,
            "Open"
                | "OpenReadOnly"
                | "OpenExisting"
                | "Exec"
                | "Query"
                | "Begin"
                | "Commit"
                | "Rollback"
                | "Changes"
                | "LastInsertRowId"
                | "Close"
        ))
    .then_some(method)
}
