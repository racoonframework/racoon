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

        // Removes two bytes read from the buffer
        buffer.drain(0..2);

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

            actual_payload_length = payload_length_to_u16(&buffer[..2])? as u64;

            // Removes used 2 bytes
            buffer.drain(0..2);
        } else {
            // For more than 126 payload length, actual size is in next 8 bytes.
            while buffer.len() < 8 {
                let chunk = stream.read_chunk().await?;
                buffer.extend(chunk);
            }

            actual_payload_length = payload_length_to_u64(&buffer[..8])?;

            // Removes used 8 bytes
            buffer.drain(0..8);
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

            // Removes read 4 bytes from the buffer
            buffer.drain(0..4);

            racoon_debug!("Websocket masking key: {:?}.", &masking_key);
        } else {
            racoon_debug!("Websocket masking disabled.");
            masking_key = None;
        }

        if actual_payload_length > max_payload_size {
            return Err(std::io::Error::other(
                "Payload length is more than the maximum allowed size.",
            ));
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

        if buffer.len() > actual_payload_length as usize {
            let extra_read: Vec<u8> = buffer.drain(actual_payload_length as usize..).collect();
            let _ = stream.restore_payload(&extra_read).await;
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
    fn payload_length_to_u16(bytes: &[u8]) -> std::io::Result<u16> {
        if bytes.len() != 2 {
            return Err(std::io::Error::other(format!(
                "Failed to convert payload length to u64. Bytes of size 2 is expected. But found: {}",
                bytes.len()
                )));
        }

        let mut tmp_bytes = [0; 2];
        tmp_bytes.copy_from_slice(bytes);
        Ok(u16::from_be_bytes(tmp_bytes))
    }

    ///
    /// Converts 8 bytes array to unsigned number.
    ///
    fn payload_length_to_u64(bytes: &[u8]) -> std::io::Result<u64> {
        if bytes.len() != 8 {
            return Err(std::io::Error::other(format!(
                "Failed to convert payload length to u64. Bytes of size 8 is expected. But found: {}",
                bytes.len()
            )));
        }

        let mut tmp_bytes = [0; 8];
        tmp_bytes.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(tmp_bytes))
    }

    #[cfg(test)]
    pub mod test {
        use std::sync::Arc;

        use crate::core::stream::{AbstractStream, TestStreamWrapper};
        use crate::core::websocket::frame::{builder, Frame};

        #[tokio::test]
        async fn test_read_single_frame() {
            let frame = Frame {
                fin: 1,
                op_code: 1,
                payload: "Hello World".as_bytes().to_vec(),
            };

            let frame_bytes = builder::build(&frame);

            let test_stream_wrapper = TestStreamWrapper::new(frame_bytes, 1024);
            let stream: Arc<Box<dyn AbstractStream + 'static>> =
                Arc::new(Box::new(test_stream_wrapper));
            let result = super::read_frame(stream, 500).await;

            assert_eq!(true, result.is_ok());
            let decoded_frame = result.unwrap();

            assert_eq!(frame.fin, decoded_frame.fin);
            assert_eq!(frame.op_code, decoded_frame.op_code);
            assert_eq!(frame.payload, decoded_frame.payload);
        }

        #[tokio::test]
        async fn test_read_multiple_frames() {
            let frame = Frame {
                fin: 1,
                op_code: 1,
                payload: "Hello World".as_bytes().to_vec(),
            };

            let text_frame_bytes = builder::build_opt(&frame, true);

            let frame2 = Frame {
                fin: 1,
                op_code: 9,
                payload: "PING".as_bytes().to_vec(),
            };
            let ping_frame_bytes = builder::build_opt(&frame2, true);

            let mut multiple_frame_bytes = text_frame_bytes;
            multiple_frame_bytes.extend(&ping_frame_bytes);

            let test_stream_wrapper = TestStreamWrapper::new(multiple_frame_bytes, 1024);
            let stream: Arc<Box<dyn AbstractStream + 'static>> =
                Arc::new(Box::new(test_stream_wrapper));

            let result1 = super::read_frame(stream.clone(), 500).await;

            // Check text frame
            assert_eq!(true, result1.is_ok());
            let decoded_frame = result1.unwrap();

            assert_eq!(frame.fin, decoded_frame.fin);
            assert_eq!(frame.op_code, decoded_frame.op_code);
            assert_eq!(frame.payload, decoded_frame.payload);

            // Check ping frame
            let result2 = super::read_frame(stream, 500).await;

            // Check text frame
            assert_eq!(true, result2.is_ok());
            let decoded_frame2 = result2.unwrap();

            assert_eq!(frame2.fin, decoded_frame2.fin);
            assert_eq!(frame2.op_code, decoded_frame2.op_code);
            assert_eq!(frame2.payload, decoded_frame2.payload);
        }
    }
}

pub mod builder {
    use rand::Rng;

    use crate::core::websocket::frame::Frame;

    pub fn build_opt(frame: &Frame, mask: bool) -> Vec<u8> {
        let mut buffer: Vec<u8> = vec![];

        // Moves fin byte towards MSB
        let fin_byte = frame.fin << 7;
        let opcode_byte = frame.op_code;
        let first_byte = fin_byte | opcode_byte;
        buffer.push(first_byte);

        let actual_payload_length = frame.payload.len();

        // Calculate the length representation and push it to the buffer
        if actual_payload_length < 126 {
            let mut second_byte = actual_payload_length as u8;

            if mask {
                // Adds 1 to MSB
                second_byte = second_byte | 0b10000000;
            }

            buffer.push(second_byte);
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

        let mut payload = frame.payload.clone();

        if mask {
            let mut thread_rng = rand::thread_rng();
            let mask_bytes: [u8; 4] = thread_rng.gen();
            buffer.extend_from_slice(&mask_bytes);

            for i in 0..frame.payload.len() {
                let mask_index = i % 4;
                payload[i] =
                    (frame.payload[i] as usize ^ mask_bytes[mask_index] as usize) as u8;
            }
        }

        // Append the payload data to the buffer
        buffer.extend_from_slice(&payload);
        buffer
    }

    pub fn build(frame: &Frame) -> Vec<u8> {
        // Disables masking message for server to client.
        build_opt(frame, false)
    }

    #[cfg(test)]
    pub mod test {
        use std::sync::Arc;

        use crate::core::stream::{AbstractStream, TestStreamWrapper};
        use crate::core::websocket::frame::reader::read_frame;
        use crate::core::websocket::frame::Frame;

        use super::build_opt;

        #[tokio::test]
        async fn test_frame_build_server() {
            let frame = Frame {
                fin: 0,
                op_code: 1,
                payload: "Hello World".as_bytes().to_vec(),
            };

            let frame_bytes = build_opt(&frame, true);

            let test_stream_wrapper = TestStreamWrapper::new(frame_bytes, 1024);
            let stream: Arc<Box<dyn AbstractStream + 'static>> =
                Arc::new(Box::new(test_stream_wrapper));

            let reader = read_frame(stream, 1000).await;
            assert_eq!(true, reader.is_ok());

            let frame = reader.unwrap();
            assert_eq!(frame.fin, 0);
            assert_eq!(frame.op_code, 1);
            assert_eq!(frame.payload, "Hello World".as_bytes().to_vec());
        }
    }
}
