use std::fs::File;
use std::io::{BufReader, Cursor, Read, Write};
use chacha20poly1305::{
    aead::{generic_array::GenericArray, Aead, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce,
};

pub fn encrypt_file_xchacha20(
    input_file_path: String,
    output_file_path: String,
    padding: usize,
) -> anyhow::Result<Vec<u8>> {
    let input = File::open(input_file_path)?;
    let reader = BufReader::new(input);

    let output = File::create(output_file_path)?;

    let res = encrypt_file_xchacha20_internal(reader, output, padding);

    Ok(res.unwrap())
}

fn encrypt_file_xchacha20_internal<R: Read>(
    mut reader: R,
    mut output_file: File,
    padding: usize,
) -> anyhow::Result<Vec<u8>> {
    let key = XChaCha20Poly1305::generate_key(&mut OsRng);
    let cipher = XChaCha20Poly1305::new(&key);

    let mut chunk_index: u32 = 0;

    let chunk_size = 262144;

    let mut buffer = [0u8; 262144];

    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }

        let length = if count < chunk_size {
            count + padding
        } else {
            count
        };

        let mut nonce = XNonce::default();

        let mut foo = [0u8; 24];
        for (place, data) in foo.iter_mut().zip(chunk_index.to_le_bytes().iter()) {
            *place = *data
        }

        nonce.copy_from_slice(&foo);

        let ciphertext = cipher.encrypt(&nonce, &buffer[..length]);

        output_file.write(&ciphertext.unwrap()).unwrap();
        chunk_index = chunk_index + 1;
    }

    output_file.flush().unwrap();

    Ok(key.to_vec())
}