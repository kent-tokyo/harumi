//! Digital signature creation for PDFs.
//!
//! This module provides functionality to create PKCS#7/CMS signatures for PDF documents.
//! Requires the `digital-signature` feature.
//!
//! Signatures are built with the RustCrypto crates (`cms`, `x509-cert`, `rsa`):
//! the CMS `SignedData` is detached (`adbe.pkcs7.detached`), carries the signing
//! certificate, and signs a set of signed attributes (content type, message digest
//! and, where a clock is available, signing time) with RSA PKCS#1 v1.5 / SHA-256.

#[cfg(feature = "digital-signature")]
pub mod inner {
    use crate::Result;
    use der::asn1::{ObjectIdentifier, OctetString, SetOfVec};
    use der::{Any, Decode, Encode};
    use pkcs1::DecodeRsaPrivateKey;
    use pkcs8::DecodePrivateKey;
    use rsa::traits::PublicKeyParts;
    use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
    use sha2::{Digest, Sha256};
    use x509_cert::Certificate;
    use x509_cert::der::DecodePem;

    pub(crate) const OID_DATA: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.1");
    pub(crate) const OID_SIGNED_DATA: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.7.2");
    pub(crate) const OID_CONTENT_TYPE: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.3");
    pub(crate) const OID_MESSAGE_DIGEST: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
    pub(crate) const OID_SIGNING_TIME: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
    pub(crate) const OID_SHA256: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");
    pub(crate) const OID_RSA_ENCRYPTION: ObjectIdentifier =
        ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
    pub(crate) const OID_COMMON_NAME: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.5.4.3");

    /// Input format for X.509 certificates.
    pub enum CertificateInput {
        /// PEM-encoded X.509 certificate.
        Pem(Vec<u8>),
        /// DER-encoded X.509 certificate.
        Der(Vec<u8>),
    }

    /// Input format for private keys.
    pub enum PrivateKeyInput {
        /// PEM-encoded PKCS#1 or PKCS#8 private key.
        Pem(Vec<u8>),
        /// DER-encoded PKCS#1 or PKCS#8 private key.
        Der(Vec<u8>),
    }

    /// Options for creating a signature field in a PDF.
    pub struct SignatureFieldOptions {
        /// Field name (used in AcroForm).
        pub field_name: String,
        /// Reason for signing (stored in `/Reason` dictionary entry).
        pub reason: Option<String>,
        /// Contact information (stored in `/ContactInfo`).
        pub contact_info: Option<String>,
        /// Reserved for certification (DocMDP) signatures. Currently not applied.
        pub lock_permissions: bool,
    }

    /// Context for signing PDF documents.
    pub struct SigningContext {
        cert_der: Vec<u8>,
        certificate: Certificate,
        private_key: RsaPrivateKey,
        signer_name: String,
    }

    impl SigningContext {
        /// Create a signing context from a certificate and private key.
        ///
        /// # Errors
        ///
        /// - [`Error::InvalidCertificate`](crate::Error::InvalidCertificate) if the
        ///   certificate cannot be parsed or does not hold an RSA public key.
        /// - [`Error::InvalidPrivateKey`](crate::Error::InvalidPrivateKey) if the key cannot
        ///   be parsed or does not match the certificate's public key.
        pub fn from_cert_and_key(cert: CertificateInput, key: PrivateKeyInput) -> Result<Self> {
            let certificate = match &cert {
                CertificateInput::Pem(pem) => Certificate::from_pem(pem),
                CertificateInput::Der(der) => Certificate::from_der(der),
            }
            .map_err(|e| crate::Error::InvalidCertificate(e.to_string()))?;
            let cert_der = certificate
                .to_der()
                .map_err(|e| crate::Error::InvalidCertificate(e.to_string()))?;

            let private_key = match &key {
                PrivateKeyInput::Pem(pem) => {
                    let pem = std::str::from_utf8(pem)
                        .map_err(|e| crate::Error::InvalidPrivateKey(e.to_string()))?;
                    RsaPrivateKey::from_pkcs8_pem(pem)
                        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
                }
                PrivateKeyInput::Der(der) => RsaPrivateKey::from_pkcs8_der(der)
                    .or_else(|_| RsaPrivateKey::from_pkcs1_der(der)),
            }
            .map_err(|e| crate::Error::InvalidPrivateKey(e.to_string()))?;

            let cert_key = rsa_public_key_from_cert(&certificate).ok_or_else(|| {
                crate::Error::InvalidCertificate("certificate does not hold an RSA key".into())
            })?;
            if cert_key != private_key.to_public_key() {
                return Err(crate::Error::InvalidPrivateKey(
                    "private key does not match the certificate's public key".into(),
                ));
            }

            let signer_name =
                subject_common_name(&certificate).unwrap_or_else(|| "Unknown Signer".to_string());

            Ok(SigningContext {
                cert_der,
                certificate,
                private_key,
                signer_name,
            })
        }

        /// Get the signer's name (the certificate subject's common name).
        pub fn signer_name(&self) -> &str {
            &self.signer_name
        }

        /// Get the certificate DER bytes.
        pub fn cert_der(&self) -> &[u8] {
            &self.cert_der
        }

        /// Get a reference to the private key.
        pub fn private_key(&self) -> &RsaPrivateKey {
            &self.private_key
        }
    }

    /// Extract the RSA public key from a certificate's SubjectPublicKeyInfo.
    pub(crate) fn rsa_public_key_from_cert(cert: &Certificate) -> Option<RsaPublicKey> {
        use pkcs8::DecodePublicKey;
        let spki = cert.tbs_certificate.subject_public_key_info.to_der().ok()?;
        RsaPublicKey::from_public_key_der(&spki).ok()
    }

    /// Return the subject common name (CN) of a certificate, if present.
    pub(crate) fn subject_common_name(cert: &Certificate) -> Option<String> {
        cert.tbs_certificate
            .subject
            .0
            .iter()
            .flat_map(|rdn| rdn.0.iter())
            .find(|atv| atv.oid == OID_COMMON_NAME)
            .and_then(|atv| {
                atv.value
                    .decode_as::<der::asn1::Utf8StringRef<'_>>()
                    .map(|s| s.to_string())
                    .or_else(|_| {
                        atv.value
                            .decode_as::<der::asn1::PrintableStringRef<'_>>()
                            .map(|s| s.to_string())
                    })
                    .ok()
            })
    }

    /// SHA-256 over the two ByteRange segments `[start1, start1+len1)` and
    /// `[start2, start2+len2)`.
    ///
    /// Returns `None` when a segment lies outside `content`.
    pub(crate) fn digest_byte_range(content: &[u8], byte_range: [usize; 4]) -> Option<Vec<u8>> {
        let [s1, l1, s2, l2] = byte_range;
        let seg1 = content.get(s1..s1.checked_add(l1)?)?;
        let seg2 = content.get(s2..s2.checked_add(l2)?)?;
        let mut hasher = Sha256::new();
        hasher.update(seg1);
        hasher.update(seg2);
        Some(hasher.finalize().to_vec())
    }

    fn attribute(oid: ObjectIdentifier, value: Any) -> Result<x509_cert::attr::Attribute> {
        let mut values = SetOfVec::new();
        values
            .insert(value)
            .map_err(|e| crate::Error::SignatureFailed(e.to_string()))?;
        Ok(x509_cert::attr::Attribute { oid, values })
    }

    fn der_err(e: der::Error) -> crate::Error {
        crate::Error::SignatureFailed(e.to_string())
    }

    /// Build a detached CMS `SignedData` (DER) over `content_digest` (SHA-256).
    pub(crate) fn build_cms_signed_data(
        context: &SigningContext,
        content_digest: &[u8],
        signing_time: Option<der::DateTime>,
    ) -> Result<Vec<u8>> {
        use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
        use cms::content_info::{CmsVersion, ContentInfo};
        use cms::signed_data::{
            EncapsulatedContentInfo, SignatureValue, SignedAttributes, SignedData,
            SignerIdentifier, SignerInfo, SignerInfos,
        };
        use spki::AlgorithmIdentifierOwned;

        let sha256 = AlgorithmIdentifierOwned {
            oid: OID_SHA256,
            parameters: None,
        };

        let mut attrs = vec![
            attribute(
                OID_CONTENT_TYPE,
                Any::encode_from(&OID_DATA).map_err(der_err)?,
            )?,
            attribute(
                OID_MESSAGE_DIGEST,
                Any::encode_from(&OctetString::new(content_digest).map_err(der_err)?)
                    .map_err(der_err)?,
            )?,
        ];
        if let Some(dt) = signing_time {
            let time = x509_cert::time::Time::UtcTime(
                der::asn1::UtcTime::from_date_time(dt).map_err(der_err)?,
            );
            attrs.push(attribute(
                OID_SIGNING_TIME,
                Any::encode_from(&time).map_err(der_err)?,
            )?);
        }
        let signed_attrs = SignedAttributes::try_from(attrs).map_err(der_err)?;

        // RFC 5652 §5.4: the signature covers the DER encoding of the signed
        // attributes as a SET OF (tag 0x31), not the [0] IMPLICIT form.
        let signed_attrs_der = signed_attrs.to_der().map_err(der_err)?;
        let attrs_digest = Sha256::digest(&signed_attrs_der);
        let signature = context
            .private_key
            .sign_with_rng(
                &mut rand::thread_rng(),
                Pkcs1v15Sign::new::<Sha256>(),
                &attrs_digest,
            )
            .map_err(|e| crate::Error::SignatureFailed(e.to_string()))?;
        debug_assert_eq!(signature.len(), context.private_key.size());

        let tbs = &context.certificate.tbs_certificate;
        let signer_info = SignerInfo {
            version: CmsVersion::V1,
            sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
                issuer: tbs.issuer.clone(),
                serial_number: tbs.serial_number.clone(),
            }),
            digest_alg: sha256.clone(),
            signed_attrs: Some(signed_attrs),
            signature_algorithm: AlgorithmIdentifierOwned {
                oid: OID_RSA_ENCRYPTION,
                parameters: Some(der::asn1::Null.into()),
            },
            signature: SignatureValue::new(signature).map_err(der_err)?,
            unsigned_attrs: None,
        };

        let mut signer_infos = SetOfVec::new();
        signer_infos.insert(signer_info).map_err(der_err)?;
        let mut digest_algorithms = SetOfVec::new();
        digest_algorithms.insert(sha256).map_err(der_err)?;
        let mut certificates = SetOfVec::new();
        certificates
            .insert(CertificateChoices::Certificate(context.certificate.clone()))
            .map_err(der_err)?;

        let signed_data = SignedData {
            version: CmsVersion::V1,
            digest_algorithms,
            encap_content_info: EncapsulatedContentInfo {
                econtent_type: OID_DATA,
                econtent: None,
            },
            certificates: Some(certificates.into()),
            crls: None,
            signer_infos: SignerInfos::from(signer_infos),
        };

        ContentInfo {
            content_type: OID_SIGNED_DATA,
            content: Any::encode_from(&signed_data).map_err(der_err)?,
        }
        .to_der()
        .map_err(der_err)
    }

    /// Hash the PDF content using SHA-256 over a ByteRange.
    ///
    /// Segments that fall outside `content` are skipped.
    #[deprecated(
        since = "1.24.0",
        note = "internal helper; Document::sign_document computes the ByteRange digest itself"
    )]
    pub fn hash_pdf_content_with_byte_range(content: &[u8], byte_range: [u32; 4]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        let [s1, l1, s2, l2] = byte_range.map(|v| v as usize);
        if let Some(seg) = content.get(s1..s1.saturating_add(l1)) {
            hasher.update(seg);
        }
        if let Some(seg) = content.get(s2..s2.saturating_add(l2)) {
            hasher.update(seg);
        }
        hasher.finalize().to_vec()
    }

    /// Hash the whole PDF content using SHA-256.
    #[deprecated(
        since = "1.24.0",
        note = "a PDF signature digest must cover the ByteRange, not the whole file"
    )]
    pub fn hash_pdf_content(content: &[u8]) -> Vec<u8> {
        Sha256::digest(content).to_vec()
    }

    /// Create an RSA PKCS#1 v1.5 signature over a SHA-256 `hash` (with blinding).
    #[deprecated(
        since = "1.24.0",
        note = "internal helper; use Document::sign_document to produce a CMS signature"
    )]
    pub fn sign_hash(private_key: &RsaPrivateKey, hash: &[u8]) -> Result<Vec<u8>> {
        private_key
            .sign_with_rng(&mut rand::thread_rng(), Pkcs1v15Sign::new::<Sha256>(), hash)
            .map_err(|e| crate::Error::SignatureFailed(e.to_string()))
    }

    /// Calculate a PDF signature ByteRange `[0, X, Y, Z]`.
    #[deprecated(
        since = "1.24.0",
        note = "internal helper; Document::sign_document computes the ByteRange itself"
    )]
    pub fn calculate_byte_range(
        pre_contents_offset: u32,
        hex_string_length: u32,
        total_pdf_size: u32,
    ) -> Result<[u32; 4]> {
        if pre_contents_offset == 0 || hex_string_length == 0 {
            return Err(crate::Error::InvalidInput(
                "ByteRange offset and length must be > 0".into(),
            ));
        }
        let start2 = pre_contents_offset
            .checked_add(hex_string_length)
            .filter(|s| *s <= total_pdf_size)
            .ok_or_else(|| crate::Error::InvalidInput("ByteRange exceeds PDF size".into()))?;
        Ok([0, pre_contents_offset, start2, total_pdf_size - start2])
    }
}

#[cfg(feature = "digital-signature")]
pub use inner::*;
