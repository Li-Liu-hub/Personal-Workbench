//! Cryptographically secure local password generation.

fn secure_random_index(upper_bound: usize) -> Result<usize, getrandom::Error> {
    let bound = upper_bound as u64;
    let limit = (u32::MAX as u64 + 1) / bound * bound;
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes)?;
        let value = u32::from_le_bytes(bytes) as u64;
        if value < limit {
            return Ok((value % bound) as usize);
        }
    }
}

pub fn generate() -> Result<String, getrandom::Error> {
    const UPPERCASE: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
    const LOWERCASE: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
    const DIGITS: &[u8] = b"23456789";
    const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{}?";
    const LENGTH: usize = 20;

    let groups = [UPPERCASE, LOWERCASE, DIGITS, SYMBOLS];
    let mut password = Vec::with_capacity(LENGTH);
    for group in groups {
        password.push(group[secure_random_index(group.len())?]);
    }

    let mut alphabet = Vec::new();
    for group in groups {
        alphabet.extend_from_slice(group);
    }
    while password.len() < LENGTH {
        password.push(alphabet[secure_random_index(alphabet.len())?]);
    }
    for index in (1..password.len()).rev() {
        password.swap(index, secure_random_index(index + 1)?);
    }

    Ok(password.into_iter().map(char::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_passwords_have_all_character_groups_and_no_ambiguous_characters() {
        for _ in 0..64 {
            let password = generate().unwrap();
            assert_eq!(password.len(), 20);
            assert!(password.bytes().any(|b| b.is_ascii_uppercase()));
            assert!(password.bytes().any(|b| b.is_ascii_lowercase()));
            assert!(password.bytes().any(|b| b.is_ascii_digit()));
            assert!(password.bytes().any(|b| !b.is_ascii_alphanumeric()));
            assert!(!password.contains(['0', 'O', '1', 'l']));
        }
    }
}
