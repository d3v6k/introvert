html_path = "/Users/dev/Development/introvert/src/dashboard.html"

with open(html_path, "r") as f:
    content = f.read()

# 1. Update header span to add id
target_span = 'style="font-size: 0.9rem; font-weight: 600; color: var(--text-muted); margin-left: 0.5rem; border-left: 1px solid var(--tile-border); padding-left: 0.5rem; letter-spacing: 0.05em; font-family: \'Inter\', sans-serif;">RBN OPERATOR</span>'
replacement_span = 'id="node-name-header" style="font-size: 0.9rem; font-weight: 600; color: var(--text-muted); margin-left: 0.5rem; border-left: 1px solid var(--tile-border); padding-left: 0.5rem; letter-spacing: 0.05em; font-family: \'Inter\', sans-serif;">RBN OPERATOR</span>'

content = content.replace(target_span, replacement_span)

# 2. Update fetchStats with registry rows loading
target_js = """                document.getElementById("sqlite-pages").innerText = data.sqlite_page_count;
                document.getElementById("uptime").innerText = formatUptime(data.uptime_seconds);"""

replacement_js = """                document.getElementById("sqlite-pages").innerText = data.sqlite_page_count;
                document.getElementById("uptime").innerText = formatUptime(data.uptime_seconds);

                // Update node name in header
                if (data.node_name) {
                    document.getElementById("node-name-header").innerText = `RBN OPERATOR (${data.node_name})`;
                }

                // Update registry list
                if (data.rbn_registry) {
                    document.getElementById("rbn-count").innerText = `${data.rbn_registry.length} Nodes Registered`;
                    const rowsContainer = document.getElementById("rbn-registry-rows");
                    if (data.rbn_registry.length === 0) {
                        rowsContainer.innerHTML = `<tr><td colspan="5" style="padding: 1rem; text-align: center; color: var(--text-muted);">No other RBNs registered on-chain yet.</td></tr>`;
                    } else {
                        rowsContainer.innerHTML = data.rbn_registry.map(rbn => {
                            const statusColor = rbn.is_active ? 'var(--success-glow)' : 'var(--error-glow)';
                            const statusText = rbn.is_active ? 'ACTIVE' : 'INACTIVE';
                            return `
                                <tr style="border-bottom: 1px solid rgba(255, 255, 255, 0.03); color: var(--text-main);">
                                    <td style="padding: 0.75rem 0.5rem; font-weight: 600; color: var(--cyber-cyan); font-family: 'Inter', sans-serif;">${rbn.node_name}</td>
                                    <td style="padding: 0.75rem 0.5rem; font-family: 'JetBrains Mono', monospace; font-size: 0.75rem;">${rbn.multiaddress}</td>
                                    <td style="padding: 0.75rem 0.5rem; font-family: 'JetBrains Mono', monospace; font-size: 0.75rem; color: var(--text-muted); cursor: pointer;" title="Click to Copy" onclick="navigator.clipboard.writeText('${rbn.peer_id}'); alert('Peer ID copied!');">${rbn.peer_id.substring(0, 16)}...</td>
                                    <td style="padding: 0.75rem 0.5rem; font-family: 'JetBrains Mono', monospace; font-size: 0.75rem; color: var(--text-muted);">${rbn.operator.substring(0, 8)}...</td>
                                    <td style="padding: 0.75rem 0.5rem; text-align: center;">
                                        <span style="color: ${statusColor}; background-color: rgba(${rbn.is_active ? '16, 185, 129' : '239, 68, 68'}, 0.1); border: 1px solid rgba(${rbn.is_active ? '16, 185, 129' : '239, 68, 68'}, 0.3); padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600;">${statusText}</span>
                                    </td>
                                </tr>
                            `;
                        }).join('');
                    }
                }"""

content = content.replace(target_js, replacement_js)

with open(html_path, "w") as f:
    f.write(content)

print("Successfully updated JS telemetry parsing in dashboard.html")
