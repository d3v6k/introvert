use snow::{Builder, HandshakeState, TransportState};
use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NoiseSessionState {
    pub local_private: Vec<u8>,
    pub remote_public: Option<Vec<u8>>,
    pub is_transport: bool,
    // Note: snow doesn't easily expose symmetric keys/nonces for full transport resumption
    // without manual state machine implementation. For Phase 1.2, we prioritize
    // rapid re-handshake (Noise IK) using these persisted parameters.
}

#[derive(Default)]
pub enum NoiseState {
    Handshake(Box<HandshakeState>),
    Transport(TransportState),
    #[default]
    Placeholder,
}

pub struct NoiseSession {
    state: NoiseState,
    local_private: Vec<u8>,
    remote_public: Option<Vec<u8>>,
}

const MAX_MESSAGE_LEN: usize = 65535;

impl NoiseSession {
    pub fn initiator(local_static: &[u8], remote_static: &[u8]) -> Result<Self> {
        if remote_static.len() != 32 {
            return Err(anyhow::anyhow!("Invalid remote static key length: {}", remote_static.len()));
        }
        if remote_static.iter().all(|&b| b == 0) {
            return Err(anyhow::anyhow!("Remote static key is all zeros"));
        }

        let builder = Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
        let handshake = builder
            .local_private_key(local_static)
            .remote_public_key(remote_static)
            .build_initiator()?;
        
        Ok(Self {
            state: NoiseState::Handshake(Box::new(handshake)),
            local_private: local_static.to_vec(),
            remote_public: Some(remote_static.to_vec()),
        })
    }

    pub fn responder(local_static: &[u8]) -> Result<Self> {
        let builder = Builder::new("Noise_IK_25519_ChaChaPoly_BLAKE2s".parse().unwrap());
        let handshake = builder
            .local_private_key(local_static)
            .build_responder()?;
        
        Ok(Self {
            state: NoiseState::Handshake(Box::new(handshake)),
            local_private: local_static.to_vec(),
            remote_public: None,
        })
    }

    /// Recovers a session from persisted state.
    pub fn from_state(state: NoiseSessionState) -> Result<Self> {
        if let Some(remote_public) = &state.remote_public {
            // If it was transport, we ideally resume. For snow, we re-handshake IK
            // to ensure safety while maintaining the verified identity.
            Self::initiator(&state.local_private, remote_public)
        } else {
            Self::responder(&state.local_private)
        }
    }

    /// Exports the current session state for persistence.
    pub fn get_state(&self) -> NoiseSessionState {
        NoiseSessionState {
            local_private: self.local_private.clone(),
            remote_public: self.remote_public.clone(),
            is_transport: matches!(self.state, NoiseState::Transport(_)),
        }
    }

    pub fn send_message(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut output = vec![0u8; MAX_MESSAGE_LEN];
        
        match &mut self.state {
            NoiseState::Handshake(handshake) => {
                let len = handshake.write_message(payload, &mut output)?;
                output.truncate(len);
                
                if handshake.is_handshake_finished() {
                    let old_state = std::mem::take(&mut self.state);
                    if let NoiseState::Handshake(h) = old_state {
                        self.state = NoiseState::Transport(h.into_transport_mode()?);
                    }
                }
                Ok(output)
            }
            NoiseState::Transport(transport) => {
                let len = transport.write_message(payload, &mut output)?;
                output.truncate(len);
                Ok(output)
            }
            NoiseState::Placeholder => unreachable!(),
        }
    }

    pub fn recv_message(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        if message.is_empty() {
            return Err(anyhow::anyhow!("Received empty Noise message"));
        }

        let mut output = vec![0u8; MAX_MESSAGE_LEN]; 
        
        match &mut self.state {
            NoiseState::Handshake(handshake) => {
                let len = handshake.read_message(message, &mut output)?;
                output.truncate(len);

                if handshake.is_handshake_finished() {
                    // Update remote public key if discovered during handshake (responder side)
                    if self.remote_public.is_none() {
                        if let Some(remote_pk) = handshake.get_remote_static() {
                            self.remote_public = Some(remote_pk.to_vec());
                        }
                    }

                    let old_state = std::mem::take(&mut self.state);
                    if let NoiseState::Handshake(h) = old_state {
                        self.state = NoiseState::Transport(h.into_transport_mode()?);
                    }
                }
                Ok(output)
            }
            NoiseState::Transport(transport) => {
                let len = transport.read_message(message, &mut output)?;
                output.truncate(len);
                Ok(output)
            }
            NoiseState::Placeholder => unreachable!(),
        }
    }

    pub fn is_finished(&self) -> bool {
        match &self.state {
            NoiseState::Handshake(h) => h.is_handshake_finished(),
            NoiseState::Transport(_) => true,
            NoiseState::Placeholder => false,
        }
    }
}
