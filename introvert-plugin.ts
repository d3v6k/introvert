import { registerPlugin } from '@capacitor/core';

export interface IntrovertPlugin {
  /**
   * Initializes the core using the persistent Ed25519 identity.
   * Derives P2P Node ID, SQLCipher key, and the Introvert Token wallet.
   */
  initializeCore(options: { dbPath: string }): Promise<{ peerId: string; walletAddress: string }>;

  /**
   * Fetches the current user balance of the native Introvert Token.
   */
  getIntrovertTokenBalance(): Promise<{ balance: string }>;

  /**
   * Submits a relay request to an Anchor node in exchange for service.
   */
  submitRelayRequest(options: { targetPeerId: string; encryptedBlob: string }): Promise<{ success: boolean }>;

  /**
   * Initiates a secure, low-latency VoIP call over WebRTC.
   */
  startVoipCall(options: { recipientPeerId: string }): Promise<{ sessionId: string }>;
}

const Introvert = registerPlugin<IntrovertPlugin>('Introvert');

export default Introvert;
