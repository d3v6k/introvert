use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_remote::TrackRemote;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use libp2p::PeerId;

use crate::economy::RewardTracker;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebRtcSignal {
    pub signal_type: String, // "offer" or "answer"
    pub sdp: String,
}

#[repr(C)]
pub struct MediaFrameHeader {
    pub codec: u8, // 0 = VP8, 1 = Opus
    pub width: u32,
    pub height: u32,
    pub timestamp: u64,
}

pub struct MediaManager;

impl MediaManager {
    pub async fn create_peer_connection(
        is_caller: bool,
        reward_tracker: Arc<RewardTracker>,
        remote_peer_id: PeerId,
    ) -> Result<Arc<RTCPeerConnection>> {
        let mut m = MediaEngine::default();
        
        m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "audio/opus".to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 111,
                ..Default::default()
            },
            RTPCodecType::Audio,
        )?;

        m.register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/VP8".to_owned(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_owned(),
                    rtcp_feedback: vec![],
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecType::Video,
        )?;

        let api = APIBuilder::new()
            .with_media_engine(m)
            .build();

        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let pc = Arc::new(api.new_peer_connection(config).await?);

        // Handle incoming tracks (Event Type 5)
        let tracker_clone = Arc::clone(&reward_tracker);
        let consumer_id = remote_peer_id.to_string();
        pc.on_track(Box::new(move |track: Arc<TrackRemote>, _receiver, _transceiver| {
            let tracker = Arc::clone(&tracker_clone);
            let consumer_id = consumer_id.clone();
            let mime_type = track.codec().capability.mime_type.to_lowercase();
            
            Box::pin(async move {
                println!("Remote track received: mime={}, type={}", mime_type, track.kind());
                
                while let Ok((rtp_packet, _)) = track.read_rtp().await {
                    let packet_len = rtp_packet.payload.len() as u64;
                    tracker.record_relay(&consumer_id, packet_len);
                    
                    let is_video = mime_type.contains("video");
                    let header = MediaFrameHeader {
                        codec: if is_video { 0 } else { 1 },
                        width: if is_video { 1280 } else { 0 }, 
                        height: if is_video { 720 } else { 0 },
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    };

                    let header_size = std::mem::size_of::<MediaFrameHeader>();
                    let mut data = Vec::with_capacity(header_size + rtp_packet.payload.len());
                    
                    let header_ptr = &header as *const MediaFrameHeader as *const u8;
                    let header_slice = unsafe { std::slice::from_raw_parts(header_ptr, header_size) };
                    data.extend_from_slice(header_slice);
                    data.extend_from_slice(&rtp_packet.payload);
                    
                    data.shrink_to_fit();
                    let ptr = data.as_ptr();
                    let len = data.len();
                    std::mem::forget(data);
                    
                    crate::dispatch_global_event(5, ptr, len);
                }
            })
        }));

        if is_caller {
            let dc = pc.create_data_channel("introvert-messaging", None).await?;
            Self::setup_data_channel_handlers(dc, reward_tracker, remote_peer_id).await;
        } else {
            let tracker_clone = Arc::clone(&reward_tracker);
            pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
                let tracker = Arc::clone(&tracker_clone);
                let peer_id = remote_peer_id;
                Box::pin(async move {
                    Self::setup_data_channel_handlers(dc, tracker, peer_id).await;
                })
            }));
        }

        pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            println!("Peer Connection State has changed: {}", s);
            Box::pin(async move {})
        }));

        Ok(pc)
    }

    async fn setup_data_channel_handlers(
        dc: Arc<RTCDataChannel>, 
        reward_tracker: Arc<RewardTracker>,
        remote_peer_id: PeerId,
    ) {
        let _dc_label = dc.label().to_owned();
        let tracker = reward_tracker;
        let consumer_id = remote_peer_id.to_string();
        
        dc.on_open(Box::new(move || {
            let mut data = "open".as_bytes().to_vec();
            data.shrink_to_fit();
            let ptr = data.as_ptr();
            let len = data.len();
            std::mem::forget(data);
            crate::dispatch_global_event(3, ptr, len);
            Box::pin(async move {})
        }));

        dc.on_message(Box::new(move |msg: DataChannelMessage| {
            let msg_len = msg.data.len() as u64;
            tracker.record_relay(&consumer_id, msg_len);
            let mut data = msg.data.to_vec();
            data.shrink_to_fit();
            let ptr = data.as_ptr();
            let len = data.len();
            std::mem::forget(data);
            crate::dispatch_global_event(4, ptr, len);
            Box::pin(async move {})
        }));
    }

    pub async fn create_offer(pc: Arc<RTCPeerConnection>) -> Result<String> {
        let offer = pc.create_offer(None).await?;
        let mut gather_complete = pc.gathering_complete_promise().await;
        pc.set_local_description(offer).await?;
        let _ = gather_complete.recv().await;

        if let Some(local_desc) = pc.local_description().await {
            Ok(local_desc.sdp)
        } else {
            Err(anyhow!("Failed to generate local description"))
        }
    }

    pub async fn handle_offer(
        offer_sdp: String,
        pc: Arc<RTCPeerConnection>,
    ) -> Result<String> {
        let mut desc = RTCSessionDescription::default();
        desc.sdp = offer_sdp;
        desc.sdp_type = RTCSdpType::Offer;
        pc.set_remote_description(desc).await?;

        let answer = pc.create_answer(None).await?;
        let mut gather_complete = pc.gathering_complete_promise().await;
        pc.set_local_description(answer).await?;
        let _ = gather_complete.recv().await;

        if let Some(local_desc) = pc.local_description().await {
            Ok(local_desc.sdp)
        } else {
            Err(anyhow!("Failed to generate local description"))
        }
    }

    pub async fn handle_answer(
        answer_sdp: String,
        pc: Arc<RTCPeerConnection>,
    ) -> Result<()> {
        let mut desc = RTCSessionDescription::default();
        desc.sdp = answer_sdp;
        desc.sdp_type = RTCSdpType::Answer;
        pc.set_remote_description(desc).await?;
        Ok(())
    }

    pub async fn add_media_tracks(pc: Arc<RTCPeerConnection>, media_type: u8) -> Result<()> {
        if media_type == 0 || media_type == 2 {
            let audio_track = Arc::new(TrackLocalStaticRTP::new(
                RTCRtpCodecCapability {
                    mime_type: "audio/opus".to_owned(),
                    ..Default::default()
                },
                "audio".to_owned(),
                "introvert-media".to_owned(),
            ));
            pc.add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>).await?;
        }

        if media_type == 1 || media_type == 2 {
            let video_track = Arc::new(TrackLocalStaticRTP::new(
                RTCRtpCodecCapability {
                    mime_type: "video/VP8".to_owned(),
                    ..Default::default()
                },
                "video".to_owned(),
                "introvert-media".to_owned(),
            ));
            pc.add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>).await?;
        }
        Ok(())
    }
}
