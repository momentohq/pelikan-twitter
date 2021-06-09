// Copyright 2021 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use super::super::*;
use crate::*;

use config::TimeType;

use core::slice::Windows;
use std::convert::TryFrom;

const MAX_COMMAND_LEN: usize = 16;
const MAX_KEY_LEN: usize = 250;
const MAX_BATCH_SIZE: usize = 1024;

const DEFAULT_MAX_VALUE_SIZE: usize = usize::MAX / 2;

#[derive(Copy, Clone)]
pub struct MemcacheRequestParser {
    max_value_size: usize,
    time_type: TimeType,
}

impl MemcacheRequestParser {
    pub fn new(max_value_size: usize, time_type: TimeType) -> Self {
        Self {
            max_value_size,
            time_type,
        }
    }
}

impl Default for MemcacheRequestParser {
    fn default() -> Self {
        Self {
            max_value_size: DEFAULT_MAX_VALUE_SIZE,
            time_type: config::time::DEFAULT_TIME_TYPE,
        }
    }
}

impl Parse<MemcacheRequest> for MemcacheRequestParser {
    fn parse(&self, buffer: &[u8]) -> Result<ParseOk<MemcacheRequest>, ParseError> {
        match parse_command(buffer)? {
            MemcacheCommand::Get => parse_get(buffer),
            MemcacheCommand::Gets => parse_gets(buffer),
            MemcacheCommand::Set => parse_set(buffer, false, self.max_value_size, self.time_type),
            MemcacheCommand::Add => parse_add(buffer, self.max_value_size, self.time_type),
            MemcacheCommand::Replace => parse_replace(buffer, self.max_value_size, self.time_type),
            MemcacheCommand::Cas => parse_set(buffer, true, self.max_value_size, self.time_type),
            MemcacheCommand::Delete => parse_delete(buffer),
            MemcacheCommand::Quit => {
                // TODO(bmartin): in-band control commands need to be handled
                // differently, this is a quick hack to emulate the 'quit'
                // command
                Err(ParseError::Invalid)
            }
        }
    }
}

struct ParseState<'a> {
    single_byte: Windows<'a, u8>,
    double_byte: Windows<'a, u8>,
}

impl<'a> ParseState<'a> {
    fn new(buffer: &'a [u8]) -> Self {
        let single_byte = buffer.windows(1);
        let double_byte = buffer.windows(2);
        Self {
            single_byte,
            double_byte,
        }
    }

    fn next_space(&mut self) -> Option<usize> {
        self.single_byte.position(|w| w == b" ")
    }

    fn next_crlf(&mut self) -> Option<usize> {
        self.double_byte.position(|w| w == CRLF.as_bytes())
    }
}

#[allow(clippy::unnecessary_unwrap)]
fn parse_command(buffer: &[u8]) -> Result<MemcacheCommand, ParseError> {
    let command;
    {
        let mut parse_state = ParseState::new(buffer);
        let next_crlf = parse_state.next_crlf();
        let next_space = parse_state.next_space();
        if next_crlf.is_some() && next_space.is_some() {
            let cmd_end = std::cmp::min(next_crlf.unwrap(), next_space.unwrap());
            command = MemcacheCommand::try_from(&buffer[0..cmd_end])?;
        } else if next_space.is_some() {
            let mut this_space = next_space.unwrap();
            match MemcacheCommand::try_from(&buffer[0..next_space.unwrap()])? {
                MemcacheCommand::Get | MemcacheCommand::Gets => {
                    let mut keys = 0;
                    while let Some(next_space) = parse_state.next_space() {
                        if next_space > MAX_KEY_LEN {
                            return Err(ParseError::Invalid);
                        }
                        keys += 1;
                        if keys >= MAX_BATCH_SIZE {
                            return Err(ParseError::Invalid);
                        }
                        this_space += next_space;
                    }
                    if buffer.len() > MAX_KEY_LEN + this_space {
                        return Err(ParseError::Invalid);
                    } else {
                        return Err(ParseError::Incomplete);
                    }
                }
                _ => {
                    if buffer.len() > MAX_COMMAND_LEN + MAX_KEY_LEN + 128 {
                        return Err(ParseError::Invalid);
                    } else {
                        return Err(ParseError::Incomplete);
                    }
                }
            };
        } else if next_crlf.is_some() {
            command = MemcacheCommand::try_from(&buffer[0..next_crlf.unwrap()])?;
            match command {
                MemcacheCommand::Quit => {}
                _ => {
                    return Err(ParseError::Invalid);
                }
            }
        } else if buffer.len() > MAX_COMMAND_LEN {
            return Err(ParseError::Invalid);
        } else {
            return Err(ParseError::Incomplete);
        }
    }
    Ok(command)
}

#[allow(clippy::unnecessary_wraps)]
fn parse_get(buffer: &[u8]) -> Result<ParseOk<MemcacheRequest>, ParseError> {
    let mut parse_state = ParseState::new(buffer);

    // this was already checked for when determining the command
    let line_end = parse_state.next_crlf().unwrap();
    let cmd_end = parse_state.next_space().unwrap();

    let mut previous = cmd_end + 1;
    let mut keys = Vec::new();

    // command may have multiple keys, we need to loop until we hit
    // a CRLF
    loop {
        if let Some(key_end) = parse_state.next_space() {
            if (previous + key_end) < line_end {
                if key_end > 0 {
                    if (previous + key_end) - previous > MAX_KEY_LEN {
                        return Err(ParseError::Invalid);
                    }
                    keys.push(
                        buffer[previous..(previous + key_end)]
                            .to_vec()
                            .into_boxed_slice(),
                    );
                } else {
                    return Err(ParseError::Invalid);
                }
                previous += key_end + 1;
            } else {
                if line_end > previous {
                    if line_end - previous > MAX_KEY_LEN {
                        return Err(ParseError::Invalid);
                    }
                    keys.push(buffer[previous..line_end].to_vec().into_boxed_slice());
                }
                break;
            }
        } else {
            if line_end > previous {
                if line_end - previous > MAX_KEY_LEN {
                    return Err(ParseError::Invalid);
                }
                keys.push(buffer[previous..line_end].to_vec().into_boxed_slice());
            }
            break;
        }
        if keys.len() >= MAX_BATCH_SIZE {
            return Err(ParseError::Invalid);
        }
    }

    if keys.is_empty() {
        Err(ParseError::Invalid)
    } else {
        let consumed = line_end + CRLF.len();

        let message = MemcacheRequest::Get {
            keys: keys.into_boxed_slice(),
        };

        Ok(ParseOk { message, consumed })
    }
}

fn parse_gets(buffer: &[u8]) -> Result<ParseOk<MemcacheRequest>, ParseError> {
    let request = parse_get(buffer)?;
    let consumed = request.consumed();
    let message = if let MemcacheRequest::Get { keys } = request.into_inner() {
        MemcacheRequest::Gets { keys }
    } else {
        unreachable!()
    };

    Ok(ParseOk { message, consumed })
}

fn parse_set(
    buffer: &[u8],
    cas: bool,
    max_value_size: usize,
    time_type: TimeType,
) -> Result<ParseOk<MemcacheRequest>, ParseError> {
    let mut parse_state = ParseState::new(buffer);

    // this was already checked for when determining the command
    let line_end = parse_state.next_crlf().unwrap();
    let cmd_end = parse_state.next_space().unwrap();

    // key
    let key_end = parse_state.next_space().ok_or(ParseError::Invalid)? + cmd_end + 1;
    if key_end <= cmd_end + 1 {
        return Err(ParseError::Invalid);
    }
    if key_end - (cmd_end + 1) > MAX_KEY_LEN {
        return Err(ParseError::Invalid);
    }

    // flags
    let flags_end = parse_state.next_space().ok_or(ParseError::Invalid)? + key_end + 1;
    let flags_str =
        std::str::from_utf8(&buffer[(key_end + 1)..flags_end]).map_err(|_| ParseError::Invalid)?;
    let flags = flags_str.parse().map_err(|_| ParseError::Invalid)?;

    // expiry
    let expiry_end = parse_state.next_space().ok_or(ParseError::Invalid)? + flags_end + 1;
    let expiry_str = std::str::from_utf8(&buffer[(flags_end + 1)..expiry_end])
        .map_err(|_| ParseError::Invalid)?;
    let expiry: u32 = expiry_str.parse().map_err(|_| ParseError::Invalid)?;
    let ttl = if time_type == TimeType::Unix
        || (time_type == TimeType::Memcache && expiry >= 60 * 60 * 24 * 30)
    {
        Some(expiry.saturating_sub(rustcommon_time::recent_unix()))
    } else if expiry == 0 {
        None
    } else {
        Some(expiry)
    };

    let mut noreply = false;

    let bytes_end = if cas {
        parse_state.next_space().ok_or(ParseError::Invalid)? + expiry_end + 1
    } else if let Some(next_space) = parse_state.next_space() {
        let next_space = next_space + expiry_end + 1;
        if line_end < next_space {
            line_end
        } else if line_end - next_space == 1 {
            next_space
        } else if line_end - (next_space + 1) == NOREPLY.len()
            || line_end - (next_space + 1) == NOREPLY.len() + 1
        {
            if &buffer[(next_space + 1)..=(next_space + NOREPLY.len())] == NOREPLY.as_bytes() {
                noreply = true;
                next_space
            } else {
                return Err(ParseError::Invalid);
            }
        } else {
            return Err(ParseError::Invalid);
        }
    } else {
        line_end
    };

    // this checks for malformed requests where a CRLF is at an
    // unexpected part of the request
    if (expiry_end + 1) >= bytes_end {
        return Err(ParseError::Invalid);
    }

    let bytes_str = std::str::from_utf8(&buffer[(expiry_end + 1)..bytes_end])
        .map_err(|_| ParseError::Invalid)?;
    let bytes = bytes_str
        .parse::<usize>()
        .map_err(|_| ParseError::Invalid)?;

    if bytes > max_value_size {
        return Err(ParseError::Invalid);
    }

    let cas_end = if !cas {
        None
    } else if let Some(next_space) = parse_state.next_space() {
        let next_space = next_space + bytes_end + 1;
        if line_end > next_space {
            if line_end - next_space == 1 {
                Some(next_space)
            } else if line_end - (next_space + 1) == NOREPLY.len()
                || line_end - (next_space + 1) == NOREPLY.len() + 1
            {
                if &buffer[(next_space + 1)..=(next_space + NOREPLY.len())] == NOREPLY.as_bytes() {
                    noreply = true;
                    Some(next_space)
                } else {
                    return Err(ParseError::Invalid);
                }
            } else {
                return Err(ParseError::Invalid);
            }
        } else {
            Some(line_end)
        }
    } else {
        Some(line_end)
    };

    let cas = if let Some(cas_end) = cas_end {
        if (bytes_end + 1) >= cas_end {
            return Err(ParseError::Invalid);
        }
        let cas_str = std::str::from_utf8(&buffer[(bytes_end + 1)..cas_end])
            .map_err(|_| ParseError::Invalid)?;
        Some(cas_str.parse::<u64>().map_err(|_| ParseError::Invalid)?)
    } else {
        None
    };

    let consumed = line_end + CRLF.len() + bytes + CRLF.len();
    if buffer.len() >= consumed {
        let key = buffer[(cmd_end + 1)..key_end].to_vec().into_boxed_slice();
        let value = buffer[(line_end + CRLF.len())..(line_end + CRLF.len() + bytes)]
            .to_vec()
            .into_boxed_slice();

        let entry = MemcacheEntry {
            key,
            value,
            ttl,
            flags,
            cas,
        };
        if cas.is_some() {
            Ok(ParseOk {
                message: MemcacheRequest::Cas { entry, noreply },
                consumed,
            })
        } else {
            Ok(ParseOk {
                message: MemcacheRequest::Set { entry, noreply },
                consumed,
            })
        }
    } else {
        // the buffer doesn't yet have all the bytes for the value
        Err(ParseError::Incomplete)
    }
}

fn parse_add(
    buffer: &[u8],
    max_value_size: usize,
    time_type: TimeType,
) -> Result<ParseOk<MemcacheRequest>, ParseError> {
    let request = parse_set(buffer, false, max_value_size, time_type)?;
    let consumed = request.consumed();

    let message = if let MemcacheRequest::Set { entry, noreply } = request.into_inner() {
        MemcacheRequest::Add { entry, noreply }
    } else {
        unreachable!()
    };

    Ok(ParseOk { message, consumed })
}

fn parse_replace(
    buffer: &[u8],
    max_value_size: usize,
    time_type: TimeType,
) -> Result<ParseOk<MemcacheRequest>, ParseError> {
    let request = parse_set(buffer, false, max_value_size, time_type)?;
    let consumed = request.consumed();

    let message = if let MemcacheRequest::Set { entry, noreply } = request.into_inner() {
        MemcacheRequest::Replace { entry, noreply }
    } else {
        unreachable!()
    };

    Ok(ParseOk { message, consumed })
}

fn parse_delete(buffer: &[u8]) -> Result<ParseOk<MemcacheRequest>, ParseError> {
    let mut single_byte = buffer.windows(1);
    // we already checked for this in the MemcacheParser::parse()
    let cmd_end = single_byte.position(|w| w == b" ").unwrap();

    let mut noreply = false;
    let mut double_byte = buffer.windows(CRLF.len());
    // get the position of the next space and first CRLF
    let next_space = single_byte.position(|w| w == b" ").map(|v| v + cmd_end + 1);
    let first_crlf = double_byte
        .position(|w| w == CRLF.as_bytes())
        .ok_or(ParseError::Incomplete)?;

    let key_end = if let Some(next_space) = next_space {
        // if we have both, bytes_end is before the earlier of the two
        if next_space < first_crlf {
            // validate that noreply isn't malformed
            if &buffer[(next_space + 1)..(first_crlf)] == NOREPLY.as_bytes() {
                noreply = true;
                next_space
            } else {
                return Err(ParseError::Invalid);
            }
        } else {
            first_crlf
        }
    } else {
        first_crlf
    };

    let consumed = if noreply {
        key_end + NOREPLY.len() + CRLF.len()
    } else {
        key_end + CRLF.len()
    };

    if key_end <= (cmd_end + 1) {
        return Err(ParseError::Invalid);
    }

    if key_end - (cmd_end + 1) > MAX_KEY_LEN {
        return Err(ParseError::Invalid);
    }

    let request = MemcacheRequest::Delete {
        key: buffer[(cmd_end + 1)..key_end].to_vec().into_boxed_slice(),
        noreply,
    };

    Ok(ParseOk {
        message: request,
        consumed,
    })
}
