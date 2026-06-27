use libp2p::{Swarm, PeerId, Multiaddr};
fn test_add_addr(swarm: &mut Swarm<()>, peer_id: PeerId, addr: Multiaddr) {
    swarm.add_peer_address(peer_id, addr);
}
fn main() {}