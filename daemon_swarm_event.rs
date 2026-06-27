   770	        }
   771	    }
   772	
   773	    async fn handle_swarm_event(&mut self, event: SwarmEvent<IntrovertBehaviourEvent>) -> anyhow::Result<()> {
   774	        match event {
   775	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Mdns(libp2p::mdns::Event::Discovered(list))) => {
   776	                for (peer_id, addr) in list {
   777	                    println!("mDNS discovered peer: {} at address: {}", peer_id, addr);
   778	                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
   779	                    let peer_id_str = peer_id.to_string();
   780	                    if let Ok(Some(_)) = self.storage.get_contact(&peer_id_str) {
   781	                        println!("Dialing verified peer discovered via mDNS: {}", peer_id);
   782	                        let _ = self.swarm.dial(peer_id);
   783	                    }
   784	                }
   785	            }
   786	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Autonat(autonat::Event::StatusChanged { old: _, new })) => {
   787	                println!("Reachability status changed: {:?}", new);
   788	                // PROACTIVE MESH REBUILD: If we just moved networks, re-dial bootstrap nodes
   789	                for (_, addr) in get_bootstrap_nodes() {
   790	                    let _ = self.swarm.dial(addr);
   791	                }
   792	                // Also re-dial known contacts to restore direct paths if possible
   793	                if let Ok(contacts) = self.storage.get_all_contacts() {
   794	                    for contact in contacts {
   795	                        if let Ok(pid) = contact.peer_id.parse::<PeerId>() {
   796	                            let _ = self.swarm.dial(pid);
   797	                        }
   798	                    }
   799	                }
   800	            }
   801	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
   802	                println!("Identify received from {}: Protocols={:?}", peer_id, info.protocols);
   803	                for addr in info.listen_addrs {
   804	                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
   805	                }
   806	                
   807	                // Discovery: If peer supports our protocol, they can be an anchor/relay
   808	                if info.protocols.iter().any(|p| p.to_string().contains("/introvert/signaling/1.0.0")) {
   809	                    println!("✨ Peer {} supports Introvert Signaling. Discovered as Anchor.", peer_id);
   810	                }
   811	
   812	                // Refresh view of the network
   813	                let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
   814	
   815	                if info.protocols.iter().any(|p| p.to_string().contains("/libp2p/circuit/relay/0.2.0/hop")) {
   816	                    println!("Relay node {} supports HOP. Requesting reservation...", peer_id);
   817	                    let relay_addr = libp2p::multiaddr::Multiaddr::empty()
   818	                        .with(libp2p::multiaddr::Protocol::P2p(peer_id))
   819	                        .with(libp2p::multiaddr::Protocol::P2pCircuit);
   820	                    let _ = self.swarm.listen_on(relay_addr);
   821	                }
   822	            }
   823	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RelayClient(event)) => {
   824	                match event {
   825	                    libp2p::relay::client::Event::ReservationReqAccepted { relay_peer_id, renewal, .. } => {
   826	                        println!("Relay reservation ACCEPTED by {}. Renewal: {}", relay_peer_id, renewal);
   827	                        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();
   828	                        let mut data = relay_peer_id.to_string().into_bytes();
   829	                        data.push(b':');
   830	                        data.push(1); // 1 = Relay Active
   831	                        crate::dispatch_global_event(8, &data);
   832	                        crate::dispatch_global_event(10, &[2]);
   833	                    }
   834	                    _ => { println!("RelayClient event: {:?}", event); }
   835	                }
   836	            }
   837	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { result: kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { key, providers, .. })), .. })) => {
   838	                let key_str = String::from_utf8_lossy(key.as_ref()).into_owned();
   839	                self.active_providers.insert(key_str.clone(), providers.iter().cloned().collect());
   840	                for peer_id in providers {
   841	                    if !self.swarm.is_connected(&peer_id) { let _ = self.swarm.dial(peer_id); }
   842	                }
   843	            }
   844	            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
   845	                println!("[Swarm] Connection established with {}", peer_id);
   846	                crate::dispatch_global_event(10, &[1]);
   847	
   848	                let is_relayed = endpoint.is_relayed();
   849	                self.is_relayed_map.insert(peer_id, is_relayed);
   850	
   851	                let status: u8 = if is_relayed { 1 } else { 0 };
   852	                let mut data = peer_id.to_string().into_bytes();
   853	                data.push(b':');
   854	                data.push(status);
   855	                crate::dispatch_global_event(8, &data);
   856	                
   857	                // Flush pending messages
   858	                if let Some(payloads) = self.pending_messages.remove(&peer_id) {
   859	                    for payload in payloads {
   860	                        let _ = self.forward_to_mesh(peer_id, payload).await;
   861	                    }
   862	                }
   863	            }
   864	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer, message: request_response::Message::Request { request, channel, .. }, .. })) => {
   865	                let _ = self.swarm.behaviour_mut().request_response.send_response(channel, SignalingResponse("ACK".to_string()));
   866	                let tx = self.command_tx.clone();
   867	                let payload = request.0;
   868	                tokio::spawn(async move {
   869	                    let _ = tx.send(NetworkCommand::HandleIncomingPayload { peer_id: peer, payload }).await;
   870	                });
   871	            }
   872	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer: _, message: request_response::Message::Response { request_id, .. }, .. })) => {
   873	                self.outbound_tracker.remove(&request_id);
   874	            }
   875	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::OutboundFailure { request_id, peer, error, .. })) => {
   876	                println!("[Mesh] Outbound Request-Response FAILURE to {}: {:?}", peer, error);
   877	                if let Some((target_peer, payload)) = self.outbound_tracker.remove(&request_id) {
   878	                    self.pending_messages.entry(target_peer).or_default().push(payload);
   879	                    let is_anchor = self.active_seeders.values().any(|s| s.peer_id == peer) ||
   880	                                    self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer.to_string())).unwrap_or(false);
   881	                    if !is_anchor && target_peer == peer {
   882	                        let _ = self.swarm.disconnect_peer_id(target_peer);
   883	                    }
   884	                }
   885	            }
   886	            SwarmEvent::ConnectionClosed { peer_id, .. } => {
   887	                if !self.swarm.is_connected(&peer_id) {
   888	                    self.is_relayed_map.remove(&peer_id);
   889	                    println!("[Swarm] Connection lost with {}. Peer is now truly offline.", peer_id);
   890	                    let mut data = peer_id.to_string().into_bytes();
   891	                    data.push(b':');
   892	                    data.push(2); // 2 = Offline
   893	                    crate::dispatch_global_event(8, &data);
   894	                }
   895	            }
   896	            SwarmEvent::OutgoingConnectionError { peer_id, .. } => {
   897	                if let Some(pid) = peer_id {
   898	                    if pid == *self.swarm.local_peer_id() { return Ok(()); }
   899	                    if !self.swarm.is_connected(&pid) {
   900	                        println!("[Swarm] All paths failed for {}. Peer is truly offline.", pid);
   901	                        let mut data = pid.to_string().into_bytes();
   902	                        data.push(b':');
   903	                        data.push(2); // 2 = Offline
   904	                        crate::dispatch_global_event(8, &data);
   905	                    }
   906	                }
   907	            }
   908	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { id, result: kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))), .. })) => {
   909	                if let Some(peer_id) = self.pending_handshakes.remove(&id) {
   910	                    let remote_static_pub: [u8; 32] = record.record.value.as_slice().try_into()?;
   911	                    let mut session = NoiseSession::initiator(self.local_static_secret.to_bytes().as_slice(), &remote_static_pub)?;
   912	                    let handshake_msg = session.send_message(&[])?;
   913	                    let storage = Arc::clone(&self.storage);
   914	                    let enc_key = self.session_encryption_key;
   915	                    let session_state = session.get_state();
   916	                    tokio::spawn(async move { let _ = NetworkService::persist_session_state(storage, enc_key, peer_id, session_state).await; });
   917	                    self.swarm.behaviour_mut().request_response.send_request(&peer_id, SignalingRequest(SignalingPayload::Secure(SecureMessage::Handshake(handshake_msg))));
   918	                    self.noise_sessions.insert(peer_id, session);
   919	                }
   920	            }
   921	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::Message { peer: _, message: request_response::Message::Response { request_id, .. }, .. })) => {
   922	                // Success! The message was delivered. Remove it from the tracker.
   923	                self.outbound_tracker.remove(&request_id);
   924	            }
   925	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::OutboundFailure { request_id, peer, error, .. })) => {
   926	                println!("[Mesh] Outbound Request-Response FAILURE to {}: {:?}", peer, error);
   927	                
   928	                // If the direct push failed, the connection is actually dead (Ghost Connection). 
   929	                // Push the payload back into pending_messages to be routed through the Mailbox.
   930	                if let Some((target_peer, payload)) = self.outbound_tracker.remove(&request_id) {
   931	                    println!("[Mesh] Re-queuing failed payload for Mailbox routing...");
   932	                    self.pending_messages.entry(target_peer).or_default().push(payload);
   933	                    
   934	                    // CRITICAL FIX: Only disconnect if the failed request was sent DIRECTLY to the peer.
   935	                    // If it was sent to an Anchor, do NOT disconnect the peer, otherwise we sever the network.
   936	                    let is_anchor = self.active_seeders.values().any(|s| s.peer_id == peer) ||
   937	                                    self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer.to_string())).unwrap_or(false);
   938	                    
   939	                    if !is_anchor && target_peer == peer {
   940	                        let _ = self.swarm.disconnect_peer_id(target_peer);
   941	                    } else if is_anchor && peer == target_peer {
   942	                        // BUG 2 FIX: Ghost anchor connection — force reconnect
   943	                        println!("[Mesh] Ghost Anchor connection detected for {}. Forcing disconnect to trigger reconnect.", peer);
   944	                        let _ = self.swarm.disconnect_peer_id(peer);
   945	                    }
   946	                }
   947	            }
   948	            SwarmEvent::Behaviour(IntrovertBehaviourEvent::RequestResponse(request_response::Event::ResponseSent { .. })) => {}
   949	            SwarmEvent::ConnectionClosed { peer_id, .. } => {
   950	                if !self.swarm.is_connected(&peer_id) {
   951	                    self.is_relayed_map.remove(&peer_id);
   952	                    println!("[Swarm] Connection lost with {}. Peer is now truly offline.", peer_id);
   953	
   954	                    // Re-dial anchors to ensure mesh remains alive
   955	                    let is_anchor = self.active_seeders.values().any(|s| s.peer_id == peer_id) ||
   956	                                    self.storage.fetch_all_anchor_nodes().map(|nodes| nodes.iter().any(|n| n.peer_id == peer_id.to_string())).unwrap_or(false);
   957	                    
   958	                    if is_anchor {
   959	                        let _ = self.swarm.dial(peer_id);
   960	                    }
   961	                }
   962	            }
   963	            SwarmEvent::OutgoingConnectionError { peer_id, .. } => {
   964	                if let Some(pid) = peer_id {
   965	                    // Ignore errors for our own PeerId (loopback/relay self-dial attempts)
   966	                    if pid == *self.swarm.local_peer_id() { return Ok(()); }
   967	
   968	                    // CRITICAL FIX: Only report offline if we have ZERO active connections to this peer.
   969	                    // If a direct dial fails but we are still connected via Relay/RBN, the peer is NOT offline.
   970	                    if self.swarm.connected_peers().find(|&p| *p == pid).is_none() {
   971	                        println!("[Swarm] All paths failed for {}. Peer is truly offline.", pid);
   972	                        let mut data = pid.to_string().into_bytes();
   973	                        data.push(2); // 2 = Offline
   974	                        crate::dispatch_global_event(8, &data);
   975	                    } else {
   976	                        println!("[Swarm] A connection path failed for {}, but other paths remain active. Skipping offline status.", pid);
   977	                    }
   978	                }
   979	            }
   980	            _ => {}
