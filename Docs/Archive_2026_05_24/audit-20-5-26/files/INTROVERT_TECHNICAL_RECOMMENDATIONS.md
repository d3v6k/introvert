# Introvert Security Audit - Technical Recommendations & Implementation Guide

**Date:** May 20, 2026  
**Scope:** Detailed, code-level fixes for identified security gaps  
**Audience:** Engineering & Security Teams

---

## PRIORITY 1: SYBIL ATTACK MITIGATION (CRITICAL)

### Problem
Any node can claim unlimited rewards for services without economic cost. Attacker can:
- Create 1,000 identities on laptop
- Claim 1,000x normal rewards
- Cause hyperinflation

### Root Cause
RewardProof verification only checks:
- Ed25519 signature (proves key holder)
- Timestamp (rejects old proofs)
- No cost to create new keys

### Solution: Add Proof-of-Work

#### Implementation (Rust Side)

**File: `src/economy/mod.rs`**

```rust
use sha2::{Sha256, Digest};

const POW_DIFFICULTY_BITS: u32 = 20;  // ~1 million hashes (adjustable)

pub struct RewardProofWithPow {
    pub proof: RewardProof,
    pub nonce: u64,
    pub difficulty: u32,  // Store difficulty for verification
}

impl RewardProofWithPow {
    /// Generate proof-of-work by finding nonce where hash has required leading zeros
    pub fn compute(
        proof: RewardProof,
        difficulty: u32,
    ) -> Result<Self, anyhow::Error> {
        let required_zeros = difficulty;
        let mut nonce = 0u64;
        let max_attempts = 10_000_000u64;  // Prevent infinite loops
        
        while nonce < max_attempts {
            let pow_input = format!(
                "{}:{}:{}:{}",
                proof.provider_id,
                proof.consumer_id,
                proof.timestamp,
                nonce
            );
            
            let hash = Sha256::digest(pow_input.as_bytes());
            
            // Count leading zero bits
            let leading_zeros = count_leading_zero_bits(&hash);
            
            if leading_zeros >= required_zeros {
                return Ok(RewardProofWithPow {
                    proof,
                    nonce,
                    difficulty,
                });
            }
            
            nonce += 1;
            
            // Yield occasionally to prevent blocking
            if nonce % 100_000 == 0 {
                tokio::task::yield_now().await;
            }
        }
        
        Err(anyhow::anyhow!("Failed to find nonce within max attempts"))
    }
    
    /// Verify proof-of-work
    pub fn verify(&self) -> Result<(), String> {
        let pow_input = format!(
            "{}:{}:{}:{}",
            self.proof.provider_id,
            self.proof.consumer_id,
            self.proof.timestamp,
            self.nonce
        );
        
        let hash = Sha256::digest(pow_input.as_bytes());
        let leading_zeros = count_leading_zero_bits(&hash);
        
        if leading_zeros < self.difficulty {
            return Err(format!(
                "Insufficient proof-of-work: {} leading zeros, need {}",
                leading_zeros, self.difficulty
            ));
        }
        
        Ok(())
    }
}

fn count_leading_zero_bits(bytes: &[u8]) -> u32 {
    let mut zeros = 0u32;
    
    for byte in bytes {
        if *byte == 0 {
            zeros += 8;
        } else {
            zeros += byte.leading_zeros();
            break;
        }
    }
    
    zeros
}

// Reward claim now requires POW
pub async fn claim_rewards_with_pow(
    &self,
    proof: RewardProof,
    difficulty: u32,
) -> Result<String> {
    // Compute POW (takes ~1 second on mobile for difficulty=20)
    let pow_proof = RewardProofWithPow::compute(proof, difficulty).await?;
    
    // Verify locally before submitting
    pow_proof.verify()?;
    
    // Submit to Solana with POW attached
    self.solana_client.submit_proof_with_pow(pow_proof).await
}
```

**File: `Cargo.toml`** - Add to dependencies:
```toml
[dependencies]
sha2 = "0.10"
```

#### Difficulty Adjustment Algorithm

```rust
pub struct DifficultyManager {
    target_pow_time_secs: u64,     // e.g., 1.0 second
    current_difficulty: u32,        // bits
    last_adjustment_time: u64,
    recent_pow_times: Vec<u64>,     // circular buffer of last 100
}

impl DifficultyManager {
    pub fn adjust_difficulty(&mut self, pow_time_secs: u64) {
        self.recent_pow_times.push(pow_time_secs);
        if self.recent_pow_times.len() > 100 {
            self.recent_pow_times.remove(0);
        }
        
        if self.recent_pow_times.len() == 100 {
            let avg_time: u64 = self.recent_pow_times.iter().sum::<u64>() / 100;
            
            if avg_time > self.target_pow_time_secs * 2 {
                // POW is too fast, increase difficulty
                self.current_difficulty = std::cmp::min(
                    self.current_difficulty + 1,
                    32  // Max 32 bits
                );
            } else if avg_time < self.target_pow_time_secs / 2 {
                // POW is too slow, decrease difficulty
                self.current_difficulty = std::cmp::max(
                    self.current_difficulty - 1,
                    15  // Min 15 bits
                );
            }
        }
    }
}
```

#### Solana Program (Treasury Verification)

**File: `solana-program/lib.rs`**

```rust
use solana_program::{
    account_info::AccountInfo,
    entrypoint,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    signature::Signature,
};
use sha2::{Sha256, Digest};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct ClaimRewardArgs {
    pub provider_pubkey: Pubkey,
    pub work_bytes: u64,
    pub timestamp: u64,
    pub signature: Vec<u8>,  // Ed25519 signature
    pub pow_nonce: u64,
    pub pow_difficulty: u32,
}

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    input: &[u8],
) -> Result<(), ProgramError> {
    let args: ClaimRewardArgs = BorshDeserialize::deserialize(&mut &input[..])?;
    
    // 1. Verify POW
    verify_proof_of_work(
        &args.provider_pubkey,
        args.timestamp,
        args.pow_nonce,
        args.pow_difficulty,
    )?;
    
    // 2. Verify signature
    verify_ed25519_signature(
        &args.provider_pubkey,
        &args.signature,
        args.work_bytes,
        args.timestamp,
    )?;
    
    // 3. Check timestamp (not older than 7 days)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    if now - args.timestamp > 7 * 86400 {
        return Err(ProgramError::Custom(1));  // Proof too old
    }
    
    // 4. Claim reward
    claim_reward(
        program_id,
        accounts,
        &args.provider_pubkey,
        args.work_bytes,
    )?;
    
    Ok(())
}

fn verify_proof_of_work(
    provider_pubkey: &Pubkey,
    timestamp: u64,
    nonce: u64,
    difficulty: u32,
) -> Result<(), ProgramError> {
    let pow_input = format!("{}:{}:{}", provider_pubkey, timestamp, nonce);
    let hash = Sha256::digest(pow_input.as_bytes());
    
    let leading_zeros = count_leading_zero_bits(&hash);
    
    if leading_zeros < difficulty {
        msg!("POW verification failed: {} < {}", leading_zeros, difficulty);
        return Err(ProgramError::Custom(2));
    }
    
    Ok(())
}

fn count_leading_zero_bits(bytes: &[u8]) -> u32 {
    let mut zeros = 0u32;
    for byte in bytes {
        if *byte == 0 {
            zeros += 8;
        } else {
            zeros += byte.leading_zeros();
            break;
        }
    }
    zeros
}
```

#### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_pow_generation_and_verification() {
        let proof = RewardProof {
            provider_id: "test_provider".to_string(),
            consumer_id: "test_consumer".to_string(),
            traffic_bytes: 1000000,
            timestamp: chrono::Utc::now().timestamp() as u64,
            signature: vec![],
            work_bytes_equivalent: 1200000,
            availability_multiplier: 1.2,
        };
        
        // Generate POW with difficulty 15 (fast for testing)
        let pow_proof = RewardProofWithPow::compute(proof, 15)
            .await
            .expect("POW generation failed");
        
        // Verify
        assert!(pow_proof.verify().is_ok());
        
        // Tampering should fail
        let mut tampered = pow_proof.clone();
        tampered.nonce += 1;
        assert!(tampered.verify().is_err());
    }
    
    #[test]
    fn test_difficulty_adjustment() {
        let mut manager = DifficultyManager {
            target_pow_time_secs: 1,
            current_difficulty: 20,
            last_adjustment_time: 0,
            recent_pow_times: vec![],
        };
        
        // Simulate fast POW times (should increase difficulty)
        for _ in 0..100 {
            manager.adjust_difficulty(0);  // 0 seconds
        }
        assert!(manager.current_difficulty > 20);
    }
}
```

#### Deployment Checklist
- [ ] Implement POW generation in Rust
- [ ] Deploy updated Solana program
- [ ] Add Dart FFI wrapper for POW computation
- [ ] Test on testnet for 1 week
- [ ] Monitor POW timing on real devices
- [ ] Adjust difficulty constants based on device variance
- [ ] Deploy to mainnet

---

## PRIORITY 2: BOOTSTRAP NODE RESILIENCE (HIGH)

### Problem
Only 2 hardcoded bootstrap nodes. If both fail, new users can't join.

### Solution: Multi-Tier Bootstrap Configuration

#### Implementation

**File: `src/network/config.rs`**

```rust
use libp2p::Multiaddr;

#[derive(Clone, Debug)]
pub struct BootstrapConfig {
    pub primary_nodes: Vec<Multiaddr>,
    pub fallback_nodes: Vec<Multiaddr>,
    pub hardcoded_ips: Vec<(String, u16)>,  // IP + port as strings for embedded
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            primary_nodes: vec![
                "/dnsaddr/rbn1.introvert.network/tcp/30333/p2p/12D3KooWABCD...EF"
                    .parse()
                    .expect("Valid multiaddr"),
                "/dnsaddr/rbn2.introvert.network/tcp/30333/p2p/12D3KooWXYZ...AB"
                    .parse()
                    .expect("Valid multiaddr"),
                "/dnsaddr/rbn3.introvert.network/tcp/30333/p2p/12D3KooW1234...56"
                    .parse()
                    .expect("Valid multiaddr"),
            ],
            fallback_nodes: vec![
                "/dnsaddr/backup-rbn1.provider1.com/tcp/30333/p2p/12D3KooWQQQQ...QQ"
                    .parse()
                    .expect("Valid multiaddr"),
                "/dnsaddr/backup-rbn2.provider2.com/tcp/30333/p2p/12D3KooWRRRR...RR"
                    .parse()
                    .expect("Valid multiaddr"),
            ],
            hardcoded_ips: vec![
                ("198.51.100.1".to_string(), 30333),    // RBN1 IP
                ("198.51.100.2".to_string(), 30333),    // RBN2 IP
                ("198.51.100.3".to_string(), 30333),    // RBN3 IP
                ("198.51.100.4".to_string(), 30333),    // Backup 1 IP
            ],
        }
    }
}

pub struct BootstrapConnector {
    config: BootstrapConfig,
}

impl BootstrapConnector {
    pub fn new(config: BootstrapConfig) -> Self {
        Self { config }
    }
    
    pub async fn connect_to_bootstrap(&self, swarm: &mut Swarm<IntrovertBehaviour>) -> Result<(), String> {
        // Try primary nodes first
        for multiaddr in &self.config.primary_nodes {
            match swarm.dial(multiaddr.clone()) {
                Ok(_) => {
                    println!("✓ Connected to primary bootstrap node: {}", multiaddr);
                    return Ok(());
                }
                Err(e) => {
                    println!("✗ Failed to connect to {}: {}", multiaddr, e);
                }
            }
        }
        
        // Fall back to secondary nodes
        println!("Primary bootstrap nodes unavailable, trying fallbacks...");
        for multiaddr in &self.config.fallback_nodes {
            match swarm.dial(multiaddr.clone()) {
                Ok(_) => {
                    println!("✓ Connected to fallback bootstrap node: {}", multiaddr);
                    return Ok(());
                }
                Err(e) => {
                    println!("✗ Failed to connect to {}: {}", multiaddr, e);
                }
            }
        }
        
        // Final fallback: hardcoded IPs (useful for DNS-less recovery)
        println!("Fallback nodes unavailable, attempting hardcoded IP recovery...");
        for (ip, port) in &self.config.hardcoded_ips {
            let multiaddr = format!("/ip4/{}/tcp/{}/p2p/12D3Koo...", ip, port);
            // Parse and dial (implementation depends on libp2p version)
            match multiaddr.parse::<Multiaddr>() {
                Ok(addr) => {
                    if swarm.dial(addr.clone()).is_ok() {
                        println!("✓ Connected via hardcoded IP: {}", ip);
                        return Ok(());
                    }
                }
                Err(_) => continue,
            }
        }
        
        Err("All bootstrap nodes failed. Check network connectivity.".to_string())
    }
}
```

#### Flutter Integration

**File: `lib/src/native/introvert_client.dart`**

```dart
Future<void> initializeBootstrap() async {
    const bootstrapConfig = {
        'primaryNodes': [
            '/dnsaddr/rbn1.introvert.network/tcp/30333/p2p/12D3Koo...',
            '/dnsaddr/rbn2.introvert.network/tcp/30333/p2p/12D3Koo...',
            '/dnsaddr/rbn3.introvert.network/tcp/30333/p2p/12D3Koo...',
        ],
        'fallbackNodes': [
            '/dnsaddr/backup-rbn1.provider1.com/tcp/30333/p2p/12D3Koo...',
            '/dnsaddr/backup-rbn2.provider2.com/tcp/30333/p2p/12D3Koo...',
        ],
        'hardcodedIps': [
            '198.51.100.1:30333',
            '198.51.100.2:30333',
            '198.51.100.3:30333',
            '198.51.100.4:30333',
        ],
    };
    
    // Log bootstrap attempt
    print('[Bootstrap] Attempting connection...');
    
    try {
        final result = await compute(
            _callBootstrapFromIsolate,
            bootstrapConfig,
        );
        
        if (result['success']) {
            print('[Bootstrap] Connected to: ${result['node']}');
            _bootstrapConnected = true;
        } else {
            print('[Bootstrap] Failed: ${result['error']}');
            _showBootstrapError(result['error']);
        }
    } catch (e) {
        print('[Bootstrap] Error: $e');
    }
}

Future<Map<String, dynamic>> _callBootstrapFromIsolate(
    Map<String, dynamic> config,
) async {
    // This runs in a separate isolate to not block UI
    // Call native Rust function with config
    return {'success': true, 'node': 'rbn1.introvert.network'};
}
```

#### Deployment Plan

**Week 1:**
- [ ] Set up 3 primary RBN nodes in different cloud regions (AWS, Azure, GCP)
- [ ] Set up 2 backup RBN nodes with independent hosting
- [ ] Configure redundant DNS (use Route53 + CloudFlare)
- [ ] Hardcode IP addresses as final fallback

**Week 2:**
- [ ] Deploy new Rust code with bootstrap config
- [ ] Test failover scenarios (turn off nodes one by one)
- [ ] Verify app connects even if primary is down
- [ ] Update Flutter app with new FFI calls

**Testing Checklist:**
- [ ] Shut down RBN1 → App uses RBN2 ✓
- [ ] Shut down RBN1+RBN2 → App uses RBN3 ✓
- [ ] Shut down all primary nodes → App uses fallback nodes ✓
- [ ] Shut down all DNS nodes → App uses hardcoded IPs ✓
- [ ] Measure connection time for each tier

---

## PRIORITY 3: RATE LIMITING ON REWARD CLAIMS (HIGH)

### Problem
Consumer can submit same provider multiple times for same traffic.

### Solution: Per-Provider & Per-IP Rate Limiting

**File: `src/economy/rate_limiter.rs`**

```rust
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use libp2p::PeerId;
use std::net::IpAddr;

pub struct RewardRateLimiter {
    /// Claims per provider in last 24 hours
    claims_per_provider: HashMap<PeerId, Vec<u64>>,
    
    /// Claims per IP in last hour
    claims_per_ip: HashMap<IpAddr, Vec<u64>>,
    
    /// Configuration
    max_claims_per_provider_per_day: usize,
    max_claims_per_ip_per_hour: usize,
}

impl RewardRateLimiter {
    pub fn new() -> Self {
        Self {
            claims_per_provider: HashMap::new(),
            claims_per_ip: HashMap::new(),
            max_claims_per_provider_per_day: 100,      // Adjust based on metrics
            max_claims_per_ip_per_hour: 1000,          // Prevent IP-level DOS
        }
    }
    
    pub fn can_claim(
        &mut self,
        provider_id: &PeerId,
        consumer_ip: Option<&IpAddr>,
    ) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        // Check provider rate limit (24 hours)
        {
            let claims = self.claims_per_provider
                .entry(*provider_id)
                .or_insert_with(Vec::new);
            
            // Remove old claims (older than 24 hours)
            claims.retain(|&ts| now - ts < 86400);
            
            if claims.len() >= self.max_claims_per_provider_per_day {
                return Err(format!(
                    "Provider {} exceeded daily limit ({}/{} claims)",
                    provider_id,
                    claims.len(),
                    self.max_claims_per_provider_per_day
                ));
            }
            
            claims.push(now);
        }
        
        // Check IP rate limit (1 hour)
        if let Some(ip) = consumer_ip {
            let claims = self.claims_per_ip
                .entry(*ip)
                .or_insert_with(Vec::new);
            
            // Remove old claims (older than 1 hour)
            claims.retain(|&ts| now - ts < 3600);
            
            if claims.len() >= self.max_claims_per_ip_per_hour {
                return Err(format!(
                    "IP {} exceeded hourly limit ({}/{} claims)",
                    ip,
                    claims.len(),
                    self.max_claims_per_ip_per_hour
                ));
            }
            
            claims.push(now);
        }
        
        Ok(())
    }
    
    pub fn get_stats(&self, provider_id: &PeerId) -> Option<RateLimitStats> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        if let Some(claims) = self.claims_per_provider.get(provider_id) {
            let recent: Vec<_> = claims
                .iter()
                .filter(|&&ts| now - ts < 86400)
                .collect();
            
            return Some(RateLimitStats {
                claims_today: recent.len(),
                max_daily: self.max_claims_per_provider_per_day,
                last_claim_ago_secs: recent
                    .last()
                    .map(|&&ts| now - ts)
                    .unwrap_or(0),
            });
        }
        
        None
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub claims_today: usize,
    pub max_daily: usize,
    pub last_claim_ago_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rate_limiting() {
        let mut limiter = RewardRateLimiter::new();
        let provider = PeerId::random();
        let ip: IpAddr = "192.168.1.1".parse().unwrap();
        
        // First claim should succeed
        assert!(limiter.can_claim(&provider, Some(&ip)).is_ok());
        
        // 100 more claims should succeed
        for _ in 0..99 {
            assert!(limiter.can_claim(&provider, Some(&ip)).is_ok());
        }
        
        // 101st claim should fail
        assert!(limiter.can_claim(&provider, Some(&ip)).is_err());
    }
}
```

#### Integration with Reward Claim Flow

**File: `src/economy/mod.rs`**

```rust
pub struct RewardEngine {
    rate_limiter: Arc<RwLock<RewardRateLimiter>>,
    // ... other fields
}

impl RewardEngine {
    pub async fn claim_rewards_guarded(
        &self,
        proof: RewardProof,
        consumer_ip: Option<&IpAddr>,
    ) -> Result<String> {
        // 1. Rate limit check
        {
            let mut limiter = self.rate_limiter.write();
            let provider_id = PeerId::from_bytes(&proof.provider_id.as_bytes())?;
            limiter.can_claim(&provider_id, consumer_ip)?;
        }
        
        // 2. Verify signature
        verify_ed25519_signature(&proof)?;
        
        // 3. Verify timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now - proof.timestamp > 7 * 86400 {
            return Err("Proof too old (>7 days)".into());
        }
        
        // 4. Submit to Solana
        self.solana_client.submit_proof(proof).await
    }
}
```

#### Dashboard for Monitoring

**File: `src/monitoring/mod.rs`**

```rust
pub struct RewardMetrics {
    pub claims_per_hour: u64,
    pub claims_per_ip_distribution: HashMap<IpAddr, usize>,
    pub providers_at_limit: usize,
    pub average_traffic_per_claim: u64,
    pub suspicious_patterns: Vec<SuspiciousPattern>,
}

pub enum SuspiciousPattern {
    HighClaimRateFromIp { ip: IpAddr, claims_per_hour: u64 },
    ManyClaimsFromOneProvider { provider: PeerId, claims_today: usize },
    ClaimTrafficMismatch { provider: PeerId, claims_vs_actual_ratio: f64 },
}

pub async fn collect_metrics(engine: &RewardEngine) -> RewardMetrics {
    // Collect and analyze
    // ...
}
```

---

## PRIORITY 4: ADD SOLANA RPC FAILOVER (MEDIUM)

### Solution

**File: `src/economy/solana.rs`**

```rust
pub struct SolanaIncentiveEngine {
    endpoints: Vec<String>,
    current_endpoint_idx: Arc<AtomicUsize>,
    health_check_interval: Duration,
    last_health_checks: Arc<RwLock<HashMap<String, u64>>>,
}

impl SolanaIncentiveEngine {
    pub fn new(endpoints: Vec<String>) -> Self {
        Self {
            endpoints,
            current_endpoint_idx: Arc::new(AtomicUsize::new(0)),
            health_check_interval: Duration::from_secs(60),
            last_health_checks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    async fn get_healthy_endpoint(&self) -> Result<&str> {
        let initial_idx = self.current_endpoint_idx.load(Ordering::Relaxed);
        
        for offset in 0..self.endpoints.len() {
            let idx = (initial_idx + offset) % self.endpoints.len();
            let endpoint = &self.endpoints[idx];
            
            if self.is_healthy(endpoint).await {
                self.current_endpoint_idx.store(idx, Ordering::Relaxed);
                return Ok(endpoint);
            }
        }
        
        Err("All RPC endpoints unhealthy".into())
    }
    
    async fn is_healthy(&self, endpoint: &str) -> bool {
        // Check if endpoint is responsive
        match reqwest::Client::new()
            .post(endpoint)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getHealth"
            }))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
    
    pub async fn submit_proof(&self, proof: RewardProof) -> Result<String> {
        let endpoint = self.get_healthy_endpoint().await?;
        
        // Submit with exponential backoff
        let mut delay = Duration::from_millis(100);
        for attempt in 0..5 {
            match self.submit_to_endpoint(endpoint, &proof).await {
                Ok(tx_sig) => return Ok(tx_sig),
                Err(e) if attempt < 4 => {
                    println!("Attempt {} failed: {}, retrying...", attempt + 1, e);
                    tokio::time::sleep(delay).await;
                    delay = Duration::from_millis(delay.as_millis() as u64 * 2);
                }
                Err(e) => return Err(e),
            }
        }
        
        Err("Proof submission failed after retries".into())
    }
}
```

---

## SUMMARY TABLE

| Recommendation | Difficulty | Timeline | Priority | Impact |
|---|---|---|---|---|
| **Proof-of-Work** | Medium | 3 days | CRITICAL | Prevents Sybil inflation |
| **Bootstrap Redundancy** | Medium | 2 days | HIGH | Ensures network accessibility |
| **Rate Limiting** | Medium | 2 days | HIGH | Prevents duplicate claims |
| **RPC Failover** | Easy | 1 day | MEDIUM | Improves reward reliability |
| **Chunk HMAC** | Easy | 1 day | MEDIUM | File transfer integrity |
| **Auto-Delete Messages** | Easy | 1 day | LOW | GDPR compliance |
| **Bandwidth Caps** | Medium | 2 days | MEDIUM | DOS protection |
| **Reputation Scoring** | Hard | 5 days | LOW | Economic efficiency |

---

## ROLLOUT TIMELINE

```
Day 1:
├─ Implement Proof-of-Work
├─ Set up 3 bootstrap nodes
└─ Configure RPC failover

Day 2:
├─ Implement rate limiting
├─ Add health checks
└─ Begin testnet deployment

Day 3:
├─ Security testing
├─ Load testing
├─ Finalize configurations
└─ Soft launch to trusted users

Week 2:
├─ Monitor economic metrics
├─ Adjust POW difficulty
└─ Public mainnet launch

```

**Total engineering effort:** ~2 weeks with 3-4 engineers

---

**Document prepared by:** Security Analysis Team  
**Last updated:** May 20, 2026
