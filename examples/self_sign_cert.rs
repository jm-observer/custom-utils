use custom_utils::tls_util::{gen_rsa_key_pem_and_file, gen_valid_date};

use picky::x509::certificate::CertificateBuilder;
use picky::x509::csr::Attribute;
use picky::x509::extension::KeyUsage;
use picky::x509::name::{DirectoryName, GeneralName, NameAttr};
use picky::x509::{csr::Csr, Extension, Extensions, KeyIdGenMethod};
use picky::{hash::HashAlgorithm, oids, signature::SignatureAlgorithm};

pub fn main() {
    let (localhost_key, _) = gen_rsa_key_pem_and_file(
        "./resource/certs/self_signed/myhost_pri.key",
        "./resource/certs/self_signed/myhost_pub.key",
    )
    .unwrap();

    let mut key_usage = KeyUsage::new(3);
    key_usage.set_digital_signature(false);
    key_usage.set_content_commitment(false);
    key_usage.set_key_encipherment(false);
    let extensions = Extensions(vec![
        Extension::new_basic_constraints(None, None).into_non_critical(),
        Extension::new_key_usage(key_usage).into_non_critical(),
        Extension::new_extended_key_usage(vec![
            oids::kp_client_auth(),
            oids::kp_server_auth(),
            oids::kp_code_signing(),
        ])
        .into_non_critical(),
        // Extension::new_subject_alt_name(vec![
        //     GeneralName::new_dns_name("www.myhost.com").unwrap().into(),
        //     GeneralName::new_dns_name("myhost.com").unwrap().into(),
        // ])
        // .into_non_critical(),
    ]);
    let attr = Attribute::new_extension_request(extensions.0);

    let mut my_name = DirectoryName::new_common_name("www.myhost.cn"); // 必须是域名!!!
    my_name.add_attr(NameAttr::StateOrProvinceName, "fujian");
    my_name.add_attr(NameAttr::CountryName, "China");
    let csr = Csr::generate_with_attributes(
        my_name.clone(),
        &localhost_key,
        SignatureAlgorithm::RsaPkcs1v15(HashAlgorithm::SHA2_256),
        vec![attr],
    )
    .unwrap();
    let (from_date, to_date) = gen_valid_date(3).unwrap();
    // let signed_leaf = gen_cert_by_ca(
    //     csr,
    //     from_date,
    //     to_date,
    //     &intermediate,
    //     &intermediate_pri,
    //     "./resource/certs/localhost.crt",
    // )
    // .unwrap();

    let signed_leaf = CertificateBuilder::new()
        .validity(from_date, to_date)
        .subject_from_csr(csr)
        .inherit_extensions_from_csr_attributes(true)
        .self_signed(my_name, &localhost_key)
        .inherit_extensions_from_csr_attributes(true)
        // .issuer_cert(&ca_cert, &ca_key)
        .signature_hash_type(SignatureAlgorithm::RsaPkcs1v15(HashAlgorithm::SHA2_256))
        .key_id_gen_method(KeyIdGenMethod::SPKFullDER(HashAlgorithm::SHA2_256))
        .build()
        .unwrap();
    let leaf_pem = signed_leaf.to_pem().unwrap();
    std::fs::write("./resource/certs/self_signed/myhost.crt", leaf_pem.to_string()).unwrap();
}
