//! The 24 recovery words behind the person key (E5), BIP39 with the English list.
//!
//! One place for it, used wherever the words are typed in: by the store on a device and by
//! the interface in the browser (as WebAssembly). So the words are turned into a signature
//! right where the human enters them and never travel anywhere (H6).

use symbiot_core::Person;

/// 24 words for 32 bytes of entropy. The caller brings the randomness of its platform.
pub fn words_from_entropy(entropy: [u8; 32]) -> String {
    bip39::Mnemonic::from_entropy(&entropy)
        .expect("32 bytes are a valid length for a mnemonic")
        .to_string()
}

/// The person key behind the words; `None` if they are not 24 words with a valid checksum.
/// Spaces, line breaks and case do not matter.
pub fn person_from_words(words: &str) -> Option<Person> {
    let normal = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mnemonic = bip39::Mnemonic::parse_normalized(&normal).ok()?;
    let entropy: [u8; 32] = mnemonic.to_entropy().try_into().ok()?;
    Some(Person::from_entropy(entropy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_give_the_same_person_however_they_are_typed() {
        let words = words_from_entropy([7; 32]);
        assert_eq!(words.split(' ').count(), 24);
        let person = person_from_words(&words).unwrap().public();
        assert_eq!(person, Person::from_entropy([7; 32]).public());
        let messy = format!("  {}\n", words.to_uppercase().replace(' ', "  \n"));
        assert_eq!(person_from_words(&messy).unwrap().public(), person);
        assert_ne!(
            person_from_words(&words_from_entropy([8; 32]))
                .unwrap()
                .public(),
            person
        );
    }

    #[test]
    fn wrong_words_are_refused() {
        let words = words_from_entropy([7; 32]);
        let mut parts: Vec<&str> = words.split(' ').collect();
        assert!(
            person_from_words(&parts[..23].join(" ")).is_none(),
            "one missing"
        );
        parts.swap(0, 1);
        assert!(person_from_words(&parts.join(" ")).is_none(), "checksum");
        assert!(person_from_words("").is_none());
    }
}
