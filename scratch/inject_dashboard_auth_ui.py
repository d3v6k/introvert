html_path = "/Users/dev/Development/introvert/src/dashboard.html"

with open(html_path, "r") as f:
    content = f.read()

# 1. Add "Change GUI Password" button in Staking panel
target_reveal_btn = """                <div class="metric-row" style="margin-top: 1rem; justify-content: center;">
                    <button class="copy-btn" id="reveal-wallet-btn" onclick="openRevealModal()" style="width: 100%; background: linear-gradient(135deg, rgba(239, 68, 68, 0.15) 0%, rgba(127, 0, 255, 0.15) 100%); border: 1px solid rgba(239, 68, 68, 0.3); padding: 0.5rem; font-weight: 600; cursor: pointer; border-radius: 6px; color: #FFF; transition: all 0.2s ease;">Reveal Operator Keypair</button>
                </div>"""

replacement_reveal_btn = """                <div class="metric-row" style="margin-top: 1rem; justify-content: center;">
                    <button class="copy-btn" id="reveal-wallet-btn" onclick="openRevealModal()" style="width: 100%; background: linear-gradient(135deg, rgba(239, 68, 68, 0.15) 0%, rgba(127, 0, 255, 0.15) 100%); border: 1px solid rgba(239, 68, 68, 0.3); padding: 0.5rem; font-weight: 600; cursor: pointer; border-radius: 6px; color: #FFF; transition: all 0.2s ease;">Reveal Operator Keypair</button>
                </div>
                <div class="metric-row" style="margin-top: 0.5rem; justify-content: center;">
                    <button class="copy-btn" id="change-pwd-btn" onclick="openChangePasswordModal()" style="width: 100%; background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); padding: 0.5rem; font-weight: 600; cursor: pointer; border-radius: 6px; color: #FFF; transition: all 0.2s ease;">Change GUI Password</button>
                </div>"""

content = content.replace(target_reveal_btn, replacement_reveal_btn)

# 2. Update fetchStats with token validation
target_fetch_stats = """        async function fetchStats() {
            try {
                const response = await fetch('/api/stats');
                const data = await response.json();"""

replacement_fetch_stats = """        async function fetchStats() {
            try {
                const token = localStorage.getItem("rbn_session_token");
                if (!token) {
                    showLoginOverlay();
                    return;
                }
                const response = await fetch(`/api/stats?token=${token}`);
                if (response.status === 401) {
                    localStorage.removeItem("rbn_session_token");
                    showLoginOverlay();
                    return;
                }
                const data = await response.json();"""

content = content.replace(target_fetch_stats, replacement_fetch_stats)

# 3. Update revealOperatorKeys with token validation
target_reveal_keys = """        async function revealOperatorKeys() {
            try {
                const response = await fetch('/api/export-wallet');
                const data = await response.json();"""

replacement_reveal_keys = """        async function revealOperatorKeys() {
            try {
                const token = localStorage.getItem("rbn_session_token");
                const response = await fetch(`/api/export-wallet?token=${token}`);
                if (response.status === 401) {
                    alert("Unauthorized. Please re-login.");
                    closeRevealModal();
                    showLoginOverlay();
                    return;
                }
                const data = await response.json();"""

content = content.replace(target_reveal_keys, replacement_reveal_keys)

# 4. Insert Login and Change Password overlays just before </body>
target_closing = """    <!-- Reveal Keypair Modal Overlay -->"""

auth_overlays = """    <!-- Login Overlay -->
    <div id="login-overlay" style="display: none; position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: linear-gradient(135deg, #090B10 0%, #151922 100%); z-index: 10000; justify-content: center; align-items: center; font-family: 'Inter', sans-serif;">
        <div style="background: rgba(255, 255, 255, 0.02); border: 1px solid rgba(255, 255, 255, 0.08); border-radius: 16px; padding: 2.5rem; width: 90%; max-width: 420px; text-align: center; box-shadow: 0 20px 50px rgba(0,0,0,0.6); backdrop-filter: blur(20px);">
            <!-- Icon/Logo -->
            <div style="font-size: 3rem; margin-bottom: 1rem; filter: drop-shadow(0 0 10px var(--cyber-cyan));">🛡️</div>
            <h2 style="color: #FFF; margin: 0 0 0.5rem 0; font-size: 1.5rem; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase;">RBN Cockpit Access</h2>
            <p style="color: var(--text-muted); font-size: 0.85rem; margin-bottom: 2rem;">Authorized Node Operators Only</p>
            
            <form onsubmit="handleLogin(event)" style="display: flex; flex-direction: column; gap: 1.25rem;">
                <div style="text-align: left; display: flex; flex-direction: column; gap: 0.35rem;">
                    <label style="font-size: 0.7rem; color: var(--text-muted); text-transform: uppercase; font-weight: 600; letter-spacing: 0.05em;">Operator Password</label>
                    <input type="password" id="login-password" required placeholder="Enter password" style="background: rgba(0,0,0,0.4); border: 1px solid rgba(255,255,255,0.1); padding: 0.75rem; border-radius: 8px; color: #FFF; font-size: 0.95rem; font-family: inherit; width: 100%; box-sizing: border-box; outline: none; transition: border-color 0.2s;" onfocus="this.style.borderColor='var(--cyber-cyan)'" onblur="this.style.borderColor='rgba(255,255,255,0.1)'"/>
                </div>
                <button type="submit" style="background: linear-gradient(135deg, var(--cyber-cyan) 0%, var(--cyber-purple) 100%); border: none; padding: 0.85rem; border-radius: 8px; color: #FFF; font-weight: 700; font-size: 0.95rem; cursor: pointer; transition: opacity 0.2s; letter-spacing: 0.05em; text-transform: uppercase;" onmouseover="this.style.opacity='0.9'" onmouseout="this.style.opacity='1'">Access Dashboard</button>
            </form>
            <div style="margin-top: 1.5rem; font-size: 0.75rem; color: var(--text-muted);">
                Default Password: <code style="color: var(--cyber-cyan); background: rgba(0,0,0,0.3); padding: 0.1rem 0.3rem; border-radius: 3px;">introvert_rbn</code>
            </div>
        </div>
    </div>

    <!-- Change Password Modal Overlay -->
    <div id="change-pwd-modal" style="display: none; position: fixed; top: 0; left: 0; width: 100%; height: 100%; background-color: rgba(11, 15, 23, 0.95); z-index: 9999; justify-content: center; align-items: center; font-family: 'Inter', sans-serif;">
        <div style="background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.08); border-radius: 12px; padding: 2rem; width: 90%; max-width: 450px; text-align: center; box-shadow: 0 10px 30px rgba(0,0,0,0.5); backdrop-filter: blur(20px);">
            <h3 style="color: #FFF; margin-top: 0; font-size: 1.2rem; letter-spacing: 0.05em; text-transform: uppercase;">Change GUI Password</h3>
            <p style="color: var(--text-muted); font-size: 0.85rem; margin-bottom: 1.5rem;">Update the password used to access this dashboard.</p>
            
            <form onsubmit="handleChangePassword(event)" style="display: flex; flex-direction: column; gap: 1rem; text-align: left;">
                <div style="display: flex; flex-direction: column; gap: 0.25rem;">
                    <label style="font-size: 0.7rem; color: var(--text-muted); text-transform: uppercase; font-weight: 600;">Current Password</label>
                    <input type="password" id="pwd-current" required style="background: rgba(0,0,0,0.4); border: 1px solid rgba(255,255,255,0.1); padding: 0.6rem; border-radius: 6px; color: #FFF; width: 100%; box-sizing: border-box;" />
                </div>
                <div style="display: flex; flex-direction: column; gap: 0.25rem;">
                    <label style="font-size: 0.7rem; color: var(--text-muted); text-transform: uppercase; font-weight: 600;">New Password</label>
                    <input type="password" id="pwd-new" required style="background: rgba(0,0,0,0.4); border: 1px solid rgba(255,255,255,0.1); padding: 0.6rem; border-radius: 6px; color: #FFF; width: 100%; box-sizing: border-box;" />
                </div>
                <div style="display: flex; flex-direction: column; gap: 0.25rem;">
                    <label style="font-size: 0.7rem; color: var(--text-muted); text-transform: uppercase; font-weight: 600;">Confirm New Password</label>
                    <input type="password" id="pwd-confirm" required style="background: rgba(0,0,0,0.4); border: 1px solid rgba(255,255,255,0.1); padding: 0.6rem; border-radius: 6px; color: #FFF; width: 100%; box-sizing: border-box;" />
                </div>
                
                <div style="display: flex; gap: 0.5rem; margin-top: 1rem;">
                    <button type="submit" style="flex-grow: 1; background: var(--cyber-cyan); border: none; padding: 0.75rem; border-radius: 6px; color: #000; font-weight: 700; cursor: pointer; font-size: 0.9rem;">Update Password</button>
                    <button type="button" onclick="closeChangePasswordModal()" style="background: transparent; border: 1px solid rgba(255,255,255,0.2); padding: 0.75rem 1rem; color: var(--text-muted); border-radius: 6px; cursor: pointer; font-size: 0.9rem;">Cancel</button>
                </div>
            </form>
        </div>
    </div>

    <!-- Reveal Keypair Modal Overlay -->"""

content = content.replace(target_closing, auth_overlays)

# 5. Insert JS auth control functions
target_js_end = """        function openRevealModal() {"""

auth_js = """        function showLoginOverlay() {
            document.getElementById("login-overlay").style.display = "flex";
        }

        function hideLoginOverlay() {
            document.getElementById("login-overlay").style.display = "none";
        }

        async function handleLogin(event) {
            event.preventDefault();
            const password = document.getElementById("login-password").value;
            try {
                const response = await fetch(`/api/login?password=${encodeURIComponent(password)}`);
                if (response.ok) {
                    const data = await response.json();
                    localStorage.setItem("rbn_session_token", data.token);
                    hideLoginOverlay();
                    fetchStats();
                } else {
                    alert("Invalid password. Please try again.");
                }
            } catch (e) {
                alert("Failed to connect to RBN daemon.");
            }
        }

        function openChangePasswordModal() {
            document.getElementById("change-pwd-modal").style.display = "flex";
            document.getElementById("pwd-current").value = "";
            document.getElementById("pwd-new").value = "";
            document.getElementById("pwd-confirm").value = "";
        }

        function closeChangePasswordModal() {
            document.getElementById("change-pwd-modal").style.display = "none";
        }

        async function handleChangePassword(event) {
            event.preventDefault();
            const current = document.getElementById("pwd-current").value;
            const newPwd = document.getElementById("pwd-new").value;
            const confirmPwd = document.getElementById("pwd-confirm").value;

            if (newPwd !== confirmPwd) {
                alert("New passwords do not match!");
                return;
            }

            const token = localStorage.getItem("rbn_session_token");
            try {
                const response = await fetch(`/api/change-password?old=${encodeURIComponent(current)}&new=${encodeURIComponent(newPwd)}&token=${token}`);
                if (response.ok) {
                    alert("Password updated successfully! Please log in again.");
                    localStorage.removeItem("rbn_session_token");
                    closeChangePasswordModal();
                    showLoginOverlay();
                } else {
                    const data = await response.json();
                    alert(data.message || "Failed to update password.");
                }
            } catch (e) {
                alert("Failed to reach RBN server.");
            }
        }

        function openRevealModal() {"""

content = content.replace(target_js_end, auth_js)

with open(html_path, "w") as f:
    f.write(content)

print("Successfully injected login, password update, and session checking features in src/dashboard.html")
