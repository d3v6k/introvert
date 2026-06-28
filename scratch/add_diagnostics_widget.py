html_path = "/Users/dev/Development/introvert/src/dashboard.html"

with open(html_path, "r") as f:
    content = f.read()

# 1. Insert CSS classes just before the closing </style> tag
css_to_add = """
        /* Diagnostic Badges */
        .diagnostic-badge {
            font-size: 0.75rem;
            font-weight: 700;
            padding: 0.2rem 0.6rem;
            border-radius: 4px;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            display: inline-block;
        }
        .badge-green {
            color: var(--success-glow);
            background-color: rgba(16, 185, 129, 0.1);
            border: 1px solid rgba(16, 185, 129, 0.3);
        }
        .badge-red {
            color: var(--error-glow);
            background-color: rgba(239, 68, 68, 0.1);
            border: 1px solid rgba(239, 68, 68, 0.3);
        }
        .badge-orange {
            color: #F59E0B;
            background-color: rgba(245, 158, 11, 0.1);
            border: 1px solid rgba(245, 158, 11, 0.3);
        }
    </style>"""

content = content.replace("    </style>", css_to_add)

# 2. Insert the Diagnostics HTML Panel immediately after the hardware tile (before the logs tile)
target_hardware_end = """                <div class="metric-row">
                    <span class="metric-label">RBN Uptime</span>
                    <span class="metric-value" id="uptime">--</span>
                </div>
            </div>
        </div>"""

replacement_hardware_end = """                <div class="metric-row">
                    <span class="metric-label">RBN Uptime</span>
                    <span class="metric-value" id="uptime">--</span>
                </div>
            </div>
        </div>

        <!-- Critical Node Diagnostics Panel -->
        <div class="tile tile-diagnostics" style="grid-column: span 6;">
            <div class="tile-title">
                <span>Critical Functions Diagnostics</span>
                <span style="color: var(--cyber-cyan)">STATUS Cockpit</span>
            </div>
            <div class="metrics-list" style="margin-top: 1rem;">
                <div class="metric-row">
                    <span class="metric-label">Daemon Service Status</span>
                    <span class="diagnostic-badge badge-green" id="diag-daemon">ACTIVE</span>
                </div>
                <div class="metric-row">
                    <span class="metric-label">Solana Registry State</span>
                    <span class="diagnostic-badge" id="diag-solana">CHECKING...</span>
                </div>
                <div class="metric-row">
                    <span class="metric-label">Port 443 Inbound Link</span>
                    <span class="diagnostic-badge" id="diag-inbound">CHECKING...</span>
                </div>
                <div class="metric-row">
                    <span class="metric-label">Outbound Network Link</span>
                    <span class="diagnostic-badge" id="diag-outbound">CHECKING...</span>
                </div>
                <div class="metric-row">
                    <span class="metric-label">Storage Engine Integrity</span>
                    <span class="diagnostic-badge" id="diag-storage">CHECKING...</span>
                </div>
                <div class="metric-row">
                    <span class="metric-label">Node Core Version</span>
                    <span class="diagnostic-badge" id="diag-version">CHECKING...</span>
                </div>
            </div>
        </div>"""

content = content.replace(target_hardware_end, replacement_hardware_end)

# 3. Update the JavaScript fetchStats function to update the badge statuses
js_target = """                // Update node name in header
                if (data.node_name) {
                    document.getElementById("node-name-header").innerText = `RBN OPERATOR (${data.node_name})`;
                }"""

js_replacement = """                // Update node name in header
                if (data.node_name) {
                    document.getElementById("node-name-header").innerText = `RBN OPERATOR (${data.node_name})`;
                }

                // 3. Update Diagnostics Badges
                const solanaBadge = document.getElementById("diag-solana");
                solanaBadge.innerText = data.solana_registry_status || 'UNKNOWN';
                solanaBadge.className = 'diagnostic-badge';
                if (solanaBadge.innerText.includes("ACTIVE")) {
                    solanaBadge.classList.add("badge-green");
                } else if (solanaBadge.innerText.includes("Low Funds") || solanaBadge.innerText.includes("UNREGISTERED")) {
                    solanaBadge.classList.add("badge-orange");
                } else {
                    solanaBadge.classList.add("badge-red");
                }

                const inboundBadge = document.getElementById("diag-inbound");
                inboundBadge.innerText = data.port_443_status || 'UNKNOWN';
                inboundBadge.className = 'diagnostic-badge';
                if (inboundBadge.innerText.includes("Traffic Detected") || inboundBadge.innerText.includes("ACTIVE")) {
                    inboundBadge.classList.add("badge-green");
                } else if (inboundBadge.innerText.includes("LISTENING")) {
                    inboundBadge.classList.add("badge-orange");
                } else {
                    inboundBadge.classList.add("badge-red");
                }

                const outboundBadge = document.getElementById("diag-outbound");
                outboundBadge.innerText = data.outbound_status || 'UNKNOWN';
                outboundBadge.className = 'diagnostic-badge';
                if (outboundBadge.innerText === "CONNECTED") {
                    outboundBadge.classList.add("badge-green");
                } else {
                    outboundBadge.classList.add("badge-red");
                }

                const storageBadge = document.getElementById("diag-storage");
                storageBadge.innerText = data.db_integrity || 'UNKNOWN';
                storageBadge.className = 'diagnostic-badge';
                if (storageBadge.innerText === "HEALTHY" || storageBadge.innerText === "OK") {
                    storageBadge.classList.add("badge-green");
                } else {
                    storageBadge.classList.add("badge-red");
                }

                const versionBadge = document.getElementById("diag-version");
                versionBadge.className = 'diagnostic-badge';
                if (data.version === data.latest_version) {
                    versionBadge.innerText = `v${data.version} (LATEST)`;
                    versionBadge.classList.add("badge-green");
                } else {
                    versionBadge.innerText = `v${data.version} (UPDATE NEEDED)`;
                    versionBadge.classList.add("badge-orange");
                }"""

content = content.replace(js_target, js_replacement)

with open(html_path, "w") as f:
    f.write(content)

print("Successfully injected Node Diagnostics Panel HTML, CSS, and JS into src/dashboard.html")
