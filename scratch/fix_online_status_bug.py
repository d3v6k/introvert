html_path = "/Users/dev/Development/introvert/src/dashboard.html"

with open(html_path, "r") as f:
    content = f.read()

target_json_parse = """                const data = await response.json();

                // 1. Update text metrics"""

replacement_online_reset = """                const data = await response.json();

                // Reset status to ONLINE on successful fetch
                document.getElementById("node-status-text").innerText = "ONLINE";
                document.querySelector(".status-badge").style.borderColor = "rgba(16, 185, 129, 0.3)";
                document.querySelector(".status-badge").style.color = "var(--success-glow)";
                document.querySelector(".status-dot").style.backgroundColor = "var(--success-glow)";

                // 1. Update text metrics"""

content = content.replace(target_json_parse, replacement_online_reset)

with open(html_path, "w") as f:
    f.write(content)

print("Successfully fixed online status bug in src/dashboard.html")
