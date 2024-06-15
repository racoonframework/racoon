///
/// Protocol format:
///
/// ```markdown
///      0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///      +-+-+-+-+-------+-+-------------+-------------------------------+
///     |F|R|R|R| opcode|M| Payload len |    Extended payload length    |
///     |I|S|S|S|  (4)  |A|     (7)     |             (16/64)           |
///     |N|V|V|V|       |S|             |   (if payload len==126/127)   |
///     | |1|2|3|       |K|             |                               |
///     +-+-+-+-+-------+-+-------------+ - - - - - - - - - - - - - - - +
///     |     Extended payload length continued, if payload len == 127  |
///     + - - - - - - - - - - - - - - - +-------------------------------+
///     |                               |Masking-key, if MASK set to 1  |
///     +-------------------------------+-------------------------------+
///     | Masking-key (continued)       |          Payload Data         |
///     +-------------------------------- - - - - - - - - - - - - - - - +
///     :                     Payload Data continued ...                :
///     + - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - +
///     |                     Payload Data continued ...                |
///     +---------------------------------------------------------------+
/// ```
///
/// More information: <https://datatracker.ietf.org/doc/html/rfc6455#section-5.2>
///
pub struct Frame {
    pub fin: u8,
    pub op_code: u8,
    pub payload: Vec<u8>,
}

pub mod reader {
    use std::sync::Arc;

    use crate::core::stream::Stream;
    use crate::core::websocket::frame::Frame;

    use crate::racoon_debug;

    pub async fn read_frame(stream: Arc<Stream>, max_payload_size: u64) -> std::io::Result<Frame> {
        let mut buffer = vec![];

        // Reads first 16 bits including FIN, RSV(1, 2, 3), OPCODE and Payload length
        while buffer.len() < 2 {
            let chunk = stream.read_chunk().await?;
            buffer.extend(chunk);
        }

        let first_byte = buffer[0];
        let fin = fin_bit_to_u8(&first_byte);
        let op_code = opcode_bit_to_u8(&first_byte);

        // 1 bit mask and 7 bit payload length
        let second_byte = buffer[1];
        let mask_bit = bit_mask_to_u8(&second_byte);

        let payload_length = payload_length_to_u8(&second_byte);

        // Removes read bytes from the buffer
        buffer = (&buffer[2..]).to_vec();

        // If length is between 0-125, this is the actual length of the message else actual length is
        // set in the next 8 bytes.
        let actual_payload_length: u64;
        if payload_length < 126 {
            actual_payload_length = payload_length as u64;
        } else if payload_length == 126 {
            // For 127 payload length, actual size is in next two bytes.
            while buffer.len() < 2 {
                let chunk = stream.read_chunk().await?;
                buffer.extend(chunk);
            }

            actual_payload_length = payload_length_to_u16(&buffer[..2]) as u64;

            // Removes used bytes
            buffer = (&buffer[2..]).to_owned();
        } else {
            // For more than 126 payload length, actual size is in next 8 bytes.
            while buffer.len() < 8 {
                let chunk = stream.read_chunk().await?;
                buffer.extend(chunk);
            }

            actual_payload_length = payload_length_to_u64(&buffer[..8]);

            // Removes used bytes
            buffer = (&buffer[8..]).to_owned();
        }

        let masking_key: Option<Vec<u8>>;

        if mask_bit == 1 {
            // Bit mask bit is set to 1, so extracts masking key of 4 bytes.
            if buffer.len() < 4 {
                let chunk = stream.read_chunk().await?;
                buffer.extend(chunk);
            }

            let key = (&buffer[..4]).to_owned();
            masking_key = Some(key);

            // Removes read bytes from the buffer
            buffer = (&buffer[4..]).to_vec();
            racoon_debug!("Websocket masking key: {:?}.", &masking_key);
        } else {
            racoon_debug!("Websocket masking disabled.");
            masking_key = None;
        }

        if actual_payload_length > max_payload_size {
            return Err(std::io::Error::other("Payload length is more than the maximum allowed size."));
        }

        // Loads message bytes to the buffer
        while buffer.len() < actual_payload_length as usize {
            let chunk = stream.read_chunk().await?;
            buffer.extend(chunk);
        }

        // Decodes websocket message using masking bit
        if let Some(masking_key) = masking_key {
            // Masking key is 4 bit
            for i in 0..buffer.len() {
                let masking_byte_index = i % 4;
                buffer[i] = buffer[i] ^ &masking_key[masking_byte_index];
            }
        }

        Ok(Frame {
            fin,
            op_code,
            payload: buffer,
        })
    }

    ///
    /// Converts final bit value to unsigned number.
    ///
    fn fin_bit_to_u8(byte: &u8) -> u8 {
        byte >> 7
    }

    ///
    /// Converts 4 bit opcode to unsigned number.
    ///
    fn opcode_bit_to_u8(byte: &u8) -> u8 {
        byte & 0b00001111
    }

    ///
    /// Converts the bits value 1 or 0 to unsigned number.
    ///
    fn bit_mask_to_u8(byte: &u8) -> u8 {
        byte >> 7
    }

    ///
    /// Converts 7 bits to unsigned number.
    ///
    fn payload_length_to_u8(byte: &u8) -> u8 {
        byte & 0b01111111
    }

    ///
    /// Converts 2 bytes array to unsigned number.
    ///
    fn payload_length_to_u16(byte: &[u8]) -> u16 {
        u16::from_be_bytes(byte.try_into().unwrap())
    }

    ///
    /// Converts 8 bytes array to unsigned number.
    ///
    fn payload_length_to_u64(bytes: &[u8]) -> u64 {
        u64::from_be_bytes(bytes.try_into().unwrap())
    }
}

pub mod builder {
    use crate::core::websocket::frame::Frame;

    pub fn build(frame: Frame) -> Vec<u8> {
        let mut buffer: Vec<u8> = vec![];

        // Moves fin byte towards MSB
        let fin_byte = frame.fin << 7;
        let opcode_byte = frame.op_code;
        let first_byte = fin_byte | opcode_byte;
        buffer.push(first_byte);

        let actual_payload_length = frame.payload.len();

        // Calculate the length representation and push it to the buffer
        if actual_payload_length < 126 {
            let second_byte = actual_payload_length as u8;
            buffer.push(second_byte); // No masking
        } else if actual_payload_length < (2_usize.pow(16)) {
            // Payload length is between 126 and 65535 bytes
            buffer.push(126); // Indicates length is in next 2 bytes

            // Convert the length to 2 bytes and push them
            let length_bytes: [u8; 2] = (actual_payload_length as u16).to_be_bytes();
            buffer.extend_from_slice(&length_bytes);
        } else {
            // Payload length is greater than or equal to 65536 bytes
            buffer.push(127); // Indicates length is in next 8 bytes

            // Convert the length to 8 bytes and push them
            let length_bytes: [u8; 8] = (actual_payload_length as u64).to_be_bytes();
            buffer.extend_from_slice(&length_bytes);
        }

        // Append the payload data to the buffer
        buffer.extend(frame.payload.iter());
        buffer
    }
}
