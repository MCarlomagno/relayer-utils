//! This module contains the `ParsedEmail` struct and its implementation.

use std::collections::HashMap;

use crate::cryptos::fetch_public_key_and_verify;
use anyhow::Result;
use cfdkim::canonicalize_signed_email;
use hex;
use itertools::Itertools;
use mailparse::{parse_mail, ParsedMail};
use serde::{Deserialize, Serialize};
use zk_regex_apis::extract_substrs::{
    extract_body_hash_idxes, extract_email_addr_idxes, extract_email_domain_idxes,
    extract_from_addr_idxes, extract_message_id_idxes, extract_subject_all_idxes,
    extract_substr_idxes, extract_timestamp_idxes, extract_to_addr_idxes,
};

/// `ParsedEmail` holds the canonicalized parts of an email along with its signature and public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedEmail {
    /// The canonicalized email header.
    pub canonicalized_header: String,
    /// The canonicalized email body.
    pub canonicalized_body: String,
    /// The email signature bytes.
    pub signature: Vec<u8>,
    /// The public key bytes associated with the email.
    pub public_key: Vec<u8>,
    /// The cleaned email body.
    pub cleaned_body: String,
    /// The email headers.
    pub headers: EmailHeaders,
}

impl ParsedEmail {
    /// Creates a new `ParsedEmail` from a raw email string.
    ///
    /// This function parses the raw email, extracts and canonicalizes the header and body,
    /// and retrieves the signature and public key.
    ///
    /// # Arguments
    ///
    /// * `raw_email` - A string slice representing the raw email to be parsed.
    ///
    /// # Returns
    ///
    /// A `Result` which is either a `ParsedEmail` instance or an error if parsing fails.
    pub async fn new_from_raw_email(raw_email: &str, ignore_body_hash_check: bool) -> Result<Self> {
        // Extract all headers
        let parsed_mail = parse_mail(raw_email.as_bytes())?;
        let headers: EmailHeaders = EmailHeaders::new_from_mail(&parsed_mail);

        let public_key =
            fetch_public_key_and_verify(parsed_mail, headers.clone(), ignore_body_hash_check)
                .await?;

        // Canonicalize the signed email to separate the header, body, and signature.
        let (canonicalized_header, canonicalized_body, signature_bytes) =
            canonicalize_signed_email(raw_email.as_bytes())?;

        // Construct the `ParsedEmail` instance.
        let parsed_email = ParsedEmail {
            canonicalized_header: String::from_utf8(canonicalized_header)?, // Convert bytes to string, may return an error if not valid UTF-8.
            canonicalized_body: String::from_utf8(canonicalized_body.clone())?, // Convert bytes to string, may return an error if not valid UTF-8.
            signature: signature_bytes.into_iter().collect_vec(), // Collect the signature bytes into a vector.
            public_key,
            cleaned_body: String::from_utf8(
                remove_quoted_printable_soft_breaks(canonicalized_body).0,
            )?, // Remove quoted-printable soft breaks from the canonicalized body.
            headers,
        };

        Ok(parsed_email)
    }

    /// Creates a new `ParsedEmail` from a raw email string.
    ///
    /// This function parses the raw email, extracts and canonicalizes the header and body.
    ///
    /// # Arguments
    ///
    /// * `raw_email` - A string slice representing the raw email to be parsed.
    /// * `public_key` - The public key.
    ///
    /// # Returns
    ///
    /// A `Result` which is either a `ParsedEmail` instance or an error if parsing fails.
    pub async fn new_from_raw_email_with_public_key(
        raw_email: &str,
        public_key: Vec<u8>,
    ) -> Result<Self> {
        println!("new_from_raw_email_with_public_key");
        // Extract all headers
        let parsed_mail = parse_mail(raw_email.as_bytes())?;
        let headers: EmailHeaders = EmailHeaders::new_from_mail(&parsed_mail);

        // Canonicalize the signed email to separate the header, body, and signature.
        let (canonicalized_header, canonicalized_body, signature_bytes) =
            canonicalize_signed_email(raw_email.as_bytes())?;

        // Construct the `ParsedEmail` instance.
        let parsed_email = ParsedEmail {
            canonicalized_header: String::from_utf8(canonicalized_header)?, // Convert bytes to string, may return an error if not valid UTF-8.
            canonicalized_body: String::from_utf8(canonicalized_body.clone())?, // Convert bytes to string, may return an error if not valid UTF-8.
            signature: signature_bytes.into_iter().collect_vec(), // Collect the signature bytes into a vector.
            public_key,
            cleaned_body: String::from_utf8(
                remove_quoted_printable_soft_breaks(canonicalized_body).0,
            )?, // Remove quoted-printable soft breaks from the canonicalized body.
            headers,
        };

        Ok(parsed_email)
    }

    /// Converts the signature bytes to a hex string with a "0x" prefix.
    pub fn signature_string(&self) -> String {
        "0x".to_string() + hex::encode(&self.signature).as_str()
    }

    /// Converts the public key bytes to a hex string with a "0x" prefix.
    pub fn public_key_string(&self) -> String {
        "0x".to_string() + hex::encode(&self.public_key).as_str()
    }

    /// Extracts the 'From' address from the canonicalized email header.
    pub fn get_from_addr(&self) -> Result<String> {
        let idxes = extract_from_addr_idxes(&self.canonicalized_header)?[0];
        Ok(self.canonicalized_header[idxes.0..idxes.1].to_string())
    }

    /// Retrieves the index range of the 'From' address within the canonicalized email header.
    pub fn get_from_addr_idxes(&self) -> Result<(usize, usize)> {
        let idxes = extract_from_addr_idxes(&self.canonicalized_header)?[0];
        Ok(idxes)
    }

    /// Extracts the 'To' address from the canonicalized email header.
    pub fn get_to_addr(&self) -> Result<String> {
        let idxes = extract_to_addr_idxes(&self.canonicalized_header)?[0];
        let str = self.canonicalized_header[idxes.0..idxes.1].to_string();
        Ok(str)
    }

    /// Extracts the email domain from the 'From' address in the canonicalized email header.
    pub fn get_email_domain(&self) -> Result<String> {
        let idxes = extract_from_addr_idxes(&self.canonicalized_header)?[0];
        let from_addr = self.canonicalized_header[idxes.0..idxes.1].to_string();
        let idxes = extract_email_domain_idxes(&from_addr)?[0];
        let str = from_addr[idxes.0..idxes.1].to_string();
        Ok(str)
    }

    /// Retrieves the index range of the email domain within the 'From' address.
    pub fn get_email_domain_idxes(&self) -> Result<(usize, usize)> {
        let idxes = extract_from_addr_idxes(&self.canonicalized_header)?[0];
        let str = self.canonicalized_header[idxes.0..idxes.1].to_string();
        let idxes = extract_email_domain_idxes(&str)?[0];
        Ok(idxes)
    }

    /// Extracts the entire subject line from the canonicalized email header.
    pub fn get_subject_all(&self) -> Result<String> {
        let idxes = extract_subject_all_idxes(&self.canonicalized_header)?[0];
        let str = self.canonicalized_header[idxes.0..idxes.1].to_string();
        Ok(str)
    }

    /// Retrieves the index range of the entire subject line within the canonicalized email header.
    pub fn get_subject_all_idxes(&self) -> Result<(usize, usize)> {
        let idxes = extract_subject_all_idxes(&self.canonicalized_header)?[0];
        Ok(idxes)
    }

    /// Retrieves the index range of the body hash within the canonicalized email header.
    pub fn get_body_hash_idxes(&self) -> Result<(usize, usize)> {
        let idxes = extract_body_hash_idxes(&self.canonicalized_header)?[0];
        Ok(idxes)
    }

    /// Returns the canonicalized email body as a string.
    pub fn get_body(&self) -> Result<String> {
        Ok(self.canonicalized_body.clone())
    }

    /// Returns the cleaned email body as a string.
    pub fn get_cleaned_body(&self) -> Result<String> {
        Ok(self.cleaned_body.clone())
    }

    /// Extracts the timestamp from the canonicalized email header.
    pub fn get_timestamp(&self) -> Result<u64> {
        let idxes = extract_timestamp_idxes(&self.canonicalized_header)?[0];
        let str = &self.canonicalized_header[idxes.0..idxes.1];
        Ok(str.parse()?)
    }

    /// Retrieves the index range of the timestamp within the canonicalized email header.
    pub fn get_timestamp_idxes(&self) -> Result<(usize, usize)> {
        let idxes = extract_timestamp_idxes(&self.canonicalized_header)?[0];
        Ok(idxes)
    }

    /// Extracts the invitation code from the canonicalized email body.
    pub fn get_invitation_code(&self, ignore_body_hash_check: bool) -> Result<String> {
        let regex_config = serde_json::from_str(include_str!("../regexes/invitation_code.json"))?;
        if ignore_body_hash_check {
            let idxes = extract_substr_idxes(&self.canonicalized_header, &regex_config, false)?[0];
            let str = self.canonicalized_header[idxes.0..idxes.1].to_string();
            Ok(str)
        } else {
            let idxes = extract_substr_idxes(&self.cleaned_body, &regex_config, false)?[0];
            let str = self.cleaned_body[idxes.0..idxes.1].to_string();
            Ok(str)
        }
    }

    /// Retrieves the index range of the invitation code within the canonicalized email body.
    pub fn get_invitation_code_idxes(
        &self,
        ignore_body_hash_check: bool,
    ) -> Result<(usize, usize)> {
        let regex_config = serde_json::from_str(include_str!("../regexes/invitation_code.json"))?;
        if ignore_body_hash_check {
            let idxes = extract_substr_idxes(&self.canonicalized_header, &regex_config, false)?[0];
            Ok(idxes)
        } else {
            let idxes = extract_substr_idxes(&self.cleaned_body, &regex_config, false)?[0];
            Ok(idxes)
        }
    }

    /// Extracts the email address from the subject line of the canonicalized email header.
    pub fn get_email_addr_in_subject(&self) -> Result<String> {
        let idxes = extract_subject_all_idxes(&self.canonicalized_header)?[0];
        let subject = self.canonicalized_header[idxes.0..idxes.1].to_string();
        let idxes = extract_email_addr_idxes(&subject)?[0];
        let str = subject[idxes.0..idxes.1].to_string();
        Ok(str)
    }

    /// Retrieves the index range of the email address within the subject line of the canonicalized email header.
    pub fn get_email_addr_in_subject_idxes(&self) -> Result<(usize, usize)> {
        let idxes = extract_subject_all_idxes(&self.canonicalized_header)?[0];
        let subject = self.canonicalized_header[idxes.0..idxes.1].to_string();
        let idxes = extract_email_addr_idxes(&subject)?[0];
        Ok(idxes)
    }

    /// Extracts the message ID from the canonicalized email header.
    pub fn get_message_id(&self) -> Result<String> {
        let idxes = extract_message_id_idxes(&self.canonicalized_header)?[0];
        let str = self.canonicalized_header[idxes.0..idxes.1].to_string();
        Ok(str)
    }

    /// Extracts the command from the canonicalized email header or body.
    pub fn get_command(&self, ignore_body_hash_check: bool) -> Result<String> {
        let regex_config = serde_json::from_str(include_str!("../regexes/command.json"))?;
        if ignore_body_hash_check {
            Ok("".to_string())
        } else {
            match extract_substr_idxes(&self.canonicalized_body, &regex_config, false) {
                Ok(idxes) => {
                    let str = self.canonicalized_body[idxes[0].0..idxes[0].1].to_string();
                    Ok(str.replace("=\r\n", ""))
                }
                Err(_) => match extract_substr_idxes(&self.cleaned_body, &regex_config, false) {
                    Ok(idxes) => {
                        let str = self.cleaned_body[idxes[0].0..idxes[0].1].to_string();
                        Ok(str)
                    }
                    _ => Ok("".to_string()),
                },
            }
        }
    }

    /// Retrieves the index range of the command within the canonicalized email header or body.
    pub fn get_command_idxes(&self, ignore_body_hash_check: bool) -> Result<(usize, usize)> {
        let regex_config = serde_json::from_str(include_str!("../regexes/command.json"))?;
        if ignore_body_hash_check {
            Ok((0, 0))
        } else {
            let idxes = extract_substr_idxes(&self.cleaned_body, &regex_config, false)?[0];
            Ok(idxes)
        }
    }

    /// Returns the cleaned email body with quoted-printable soft line breaks removed.
    pub fn get_body_with_soft_line_breaks(&self) -> Result<String> {
        Ok(self.cleaned_body.clone())
    }
}

/// Removes Quoted-Printable (QP) soft line breaks (`=\r\n`) from the given byte vector while
/// maintaining a mapping from cleaned indices back to the original positions.
///
/// Quoted-printable encoding may split long lines with `=\r\n` sequences. This function removes
/// these soft line breaks, producing a "cleaned" output array. It also creates an index map so
/// that for each position in the cleaned output, you can find the corresponding original index.
///
/// Any positions in the cleaned output that were added as padding (to match the original length)
/// will have their index map entry set to `usize::MAX`, indicating no corresponding original index.
///
/// # Arguments
///
/// * `body` - A `Vec<u8>` containing the QP-encoded content.
///
/// # Returns
///
/// A tuple of:
/// - `Vec<u8>`: The cleaned content, with all QP soft line breaks removed and padded with zeros
///              to match the original length.
/// - `Vec<usize>`: A mapping from cleaned indices to original indices. For cleaned indices that
///                 correspond to actual content, `index_map[i]` gives the original position of
///                 that byte in `body`. For padded bytes, the value is `usize::MAX`.
pub fn remove_quoted_printable_soft_breaks(body: Vec<u8>) -> (Vec<u8>, Vec<usize>) {
    let original_len = body.len();
    let mut cleaned = Vec::with_capacity(original_len);
    let mut index_map = Vec::with_capacity(original_len);

    let mut iter = body.iter().enumerate();
    while let Some((i, &byte)) = iter.next() {
        // Check if this is the start of a soft line break sequence `=\r\n`
        if byte == b'=' && body.get(i + 1..i + 3) == Some(&[b'\r', b'\n']) {
            // Skip the next two bytes for the soft line break
            iter.nth(1);
        } else {
            cleaned.push(byte);
            index_map.push(i);
        }
    }

    // Pad the cleaned result with zeros to match the original length
    cleaned.resize(original_len, 0);

    // Pad index_map with usize::MAX for these padded positions
    let padding_needed = original_len - index_map.len();
    index_map.extend(std::iter::repeat(usize::MAX).take(padding_needed));

    (cleaned, index_map)
}

/// Finds the index of the first occurrence of a pattern in the given body.
///
/// This function searches for the pattern within the body and returns the index of its first occurrence.
/// If the pattern is not found or is empty, the function returns 0.
///
/// # Arguments
///
/// * `body` - An `Option` wrapping a reference to a `Vec<u8>` representing the email body.
/// * `pattern` - A string slice representing the pattern to search for.
///
/// # Returns
///
/// The index of the first occurrence of the pattern within the body as `usize`.
pub(crate) fn find_index_in_body(body: Option<&Vec<u8>>, pattern: &str) -> usize {
    body.and_then(|body_bytes| {
        if !pattern.is_empty() {
            // Search for the pattern in the body
            body_bytes
                .windows(pattern.len())
                .position(|w| w == pattern.as_bytes())
        } else {
            None
        }
    })
    .unwrap_or(0) // Default to 0 if not found or pattern is empty
}

/// Represents the email headers as a collection of key-value pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailHeaders(HashMap<String, Vec<String>>);

impl EmailHeaders {
    /// Creates a new `EmailHeaders` instance from a parsed email.
    ///
    /// # Arguments
    ///
    /// * `parsed_mail` - A reference to a `ParsedMail` instance.
    ///
    /// # Returns
    ///
    /// A new `EmailHeaders` instance containing the headers from the parsed email.
    pub fn new_from_mail(parsed_mail: &ParsedMail) -> Self {
        let mut headers = HashMap::new();
        for header in &parsed_mail.headers {
            let key = header.get_key().to_string();
            let value = header.get_value();
            headers.entry(key).or_insert_with(Vec::new).push(value);
        }
        Self(headers)
    }

    /// Retrieves the value(s) of a specific header.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the header to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option` containing a `Vec<String>` of header values if the header exists, or `None` if it doesn't.
    pub fn get_header(&self, name: &str) -> Option<Vec<String>> {
        self.0.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[tokio::test]
    async fn test_new_from_raw_email() -> Result<()> {
        if std::env::var("CI").is_ok() {
            println!("Skipping test that requires confidential data in CI environment");
            return Ok(());
        }

        let test_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("confidential")
            .join("amazon.eml");

        let raw_email = fs::read_to_string(test_file)?;

        // Run in a loop, because with multiple keys returned it should always find the correct one
        for i in 0..10 {
            println!("i: {:?}", i);
            let parsed_email = ParsedEmail::new_from_raw_email(&raw_email, true).await?;
            assert!(!parsed_email.canonicalized_header.is_empty());
            assert!(!parsed_email.canonicalized_body.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_new_from_raw_email_gappssmtp() -> Result<()> {
        if std::env::var("CI").is_ok() {
            println!("Skipping test that requires confidential data in CI environment");
            return Ok(());
        }

        let test_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("confidential")
            .join("berkley.eml");

        let raw_email = fs::read_to_string(test_file)?;

        let parsed_email = ParsedEmail::new_from_raw_email(&raw_email, true).await?;
        assert!(!parsed_email.canonicalized_header.is_empty());
        assert!(!parsed_email.canonicalized_body.is_empty());
        Ok(())
    }
}
