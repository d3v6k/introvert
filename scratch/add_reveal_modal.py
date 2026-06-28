html_path = "/Users/dev/Development/introvert/src/dashboard.html"

with open(html_path, "r") as f:
    content = f.read()

# 1. Add the Reveal Button in the Staking Panel
target_staking_end = """                <div class="metric-row">
                    <span class="metric-label">Lease Status</span>
                    <span class="metric-value" id="lease-status">VALID</span>
                </div>
            </div>
        </div>"""

replacement_staking_end = """                <div class="metric-row">
                    <span class="metric-label">Lease Status</span>
                    <span class="metric-value" id="lease-status">VALID</span>
                </div>
                <div class="metric-row" style="margin-top: 1rem; justify-content: center;">
                    <button class="copy-btn" id="reveal-wallet-btn" onclick="openRevealModal()" style="width: 100%; background: linear-gradient(135deg, rgba(239, 68, 68, 0.15) 0%, rgba(127, 0, 255, 0.15) 100%); border: 1px solid rgba(239, 68, 68, 0.3); padding: 0.5rem; font-weight: 600; cursor: pointer; border-radius: 6px; color: #FFF; transition: all 0.2s ease;">Reveal Operator Keypair</button>
                </div>
            </div>
        </div>"""

content = content.replace(target_staking_end, replacement_staking_end)

# 2. Add the Modal HTML Overlay just before the closing </body> tag
target_body_end = """    </script>
</body>
</html>"""

modal_html = """    <!-- Reveal Keypair Modal Overlay -->
    <div id="reveal-modal" style="display: none; position: fixed; top: 0; left: 0; width: 100%; height: 100%; background-color: rgba(11, 15, 23, 0.95); z-index: 9999; justify-content: center; align-items: center; font-family: 'Inter', sans-serif;">
        <div style="background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.08); border-radius: 12px; padding: 2rem; width: 90%; max-width: 550px; text-align: center; box-shadow: 0 10px 30px rgba(0,0,0,0.5); backdrop-filter: blur(20px);">
            <h3 style="color: var(--error-glow); margin-top: 0; font-size: 1.3rem; letter-spacing: 0.05em; display: flex; align-items: center; justify-content: center; gap: 0.5rem;">
                ⚠️ CRITICAL SECURITY WARNING
            </h3>
            <p style="color: var(--text-muted); font-size: 0.9rem; line-height: 1.5; margin: 1rem 0;">
                You are about to export the private key material of your RBN Operator wallet. Anyone who obtains this key will have complete ownership of your on-chain funds.
            </p>
            <div style="background-color: rgba(239, 68, 68, 0.1); border: 1px solid rgba(239, 68, 68, 0.3); border-radius: 6px; padding: 0.75rem; color: #FFF; font-size: 0.85rem; font-weight: 600; text-align: left; margin-bottom: 1.5rem;">
                Make sure no one is looking at your screen, and never share this private key with anyone, including the Introvert core team.
            </div>
            
            <div id="modal-initial-actions">
                <button onclick="revealOperatorKeys()" style="background-color: var(--error-glow); border: none; padding: 0.75rem 1.5rem; font-weight: 700; color: #FFF; border-radius: 6px; cursor: pointer; font-size: 0.9rem; margin-right: 0.5rem; transition: background-color 0.2s;">I Understand, Reveal Keys</button>
                <button onclick="closeRevealModal()" style="background-color: transparent; border: 1px solid rgba(255,255,255,0.2); padding: 0.75rem 1.5rem; font-weight: 600; color: var(--text-muted); border-radius: 6px; cursor: pointer; font-size: 0.9rem; transition: all 0.2s;">Cancel</button>
            </div>

            <div id="modal-key-display" style="display: none; text-align: left; margin-top: 1.5rem; gap: 1rem; flex-direction: column;">
                <div style="display: flex; flex-direction: column; gap: 0.25rem;">
                    <label style="font-size: 0.75rem; color: var(--text-muted); text-transform: uppercase; font-weight: 600;">Base58 Private Key (Phantom / Solflare)</label>
                    <div style="display: flex; gap: 0.5rem; align-items: center;">
                        <input type="password" id="modal-priv-b58" readonly style="flex-grow: 1; background: rgba(0,0,0,0.5); border: 1px solid rgba(255,255,255,0.1); padding: 0.5rem; border-radius: 4px; color: var(--cyber-cyan); font-family: 'JetBrains Mono', monospace; font-size: 0.8rem;" />
                        <button onclick="toggleVisibility('modal-priv-b58')" class="copy-btn">Show</button>
                        <button onclick="copyElementText('modal-priv-b58', 'Private Key')" class="copy-btn">Copy</button>
                    </div>
                </div>
                <div style="display: flex; flex-direction: column; gap: 0.25rem; margin-top: 1rem;">
                    <label style="font-size: 0.75rem; color: var(--text-muted); text-transform: uppercase; font-weight: 600;">Solana CLI JSON Keypair Array</label>
                    <div style="display: flex; gap: 0.5rem; align-items: center;">
                        <input type="password" id="modal-priv-json" readonly style="flex-grow: 1; background: rgba(0,0,0,0.5); border: 1px solid rgba(255,255,255,0.1); padding: 0.5rem; border-radius: 4px; color: var(--cyber-purple); font-family: 'JetBrains Mono', monospace; font-size: 0.8rem;" />
                        <button onclick="toggleVisibility('modal-priv-json')" class="copy-btn">Show</button>
                        <button onclick="copyElementText('modal-priv-json', 'JSON Keypair')" class="copy-btn">Copy</button>
                    </div>
                </div>
                <button onclick="closeRevealModal()" style="margin-top: 1.5rem; width: 100%; background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); padding: 0.75rem; border-radius: 6px; color: #FFF; font-weight: 600; cursor: pointer; transition: background 0.2s;">Done & Lock Wallet</button>
            </div>
        </div>
    </div>

    <script>
        function openRevealModal() {
            document.getElementById("reveal-modal").style.display = "flex";
            document.getElementById("modal-initial-actions").style.display = "block";
            document.getElementById("modal-key-display").style.display = "none";
            document.getElementById("modal-priv-b58").value = "";
            document.getElementById("modal-priv-json").value = "";
            document.getElementById("modal-priv-b58").type = "password";
            document.getElementById("modal-priv-json").type = "password";
        }

        function closeRevealModal() {
            document.getElementById("reveal-modal").style.display = "none";
            document.getElementById("modal-priv-b58").value = "";
            document.getElementById("modal-priv-json").value = "";
        }

        async function revealOperatorKeys() {
            try {
                const response = await fetch('/api/export-wallet');
                const data = await response.json();
                document.getElementById("modal-priv-b58").value = data.private_key_base58;
                document.getElementById("modal-priv-json").value = data.private_key_json;
                document.getElementById("modal-initial-actions").style.display = "none";
                document.getElementById("modal-key-display").style.display = "flex";
            } catch (e) {
                alert("Failed to export key material. Ensure daemon is running.");
            }
        }

        function toggleVisibility(id) {
            const input = document.getElementById(id);
            if (input.type === "password") {
                input.type = "text";
            } else {
                input.type = "password";
            }
        }

        function copyElementText(id, label) {
            const input = document.getElementById(id);
            navigator.clipboard.writeText(input.value);
            alert(`${label} copied to clipboard!`);
        }
    </script>
</body>
</html>"""

content = content.replace("</body>\n</html>", modal_html)

with open(html_path, "w") as f:
    f.write(content)

print("Successfully injected secure Reveal Wallet modal into src/dashboard.html")
