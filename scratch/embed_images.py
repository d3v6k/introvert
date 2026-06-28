import base64
import os

html_path = "/Users/dev/Development/introvert/src/dashboard.html"
logo_path = "/Users/dev/Documents/introvert logo/introvert logo white.png"
icon_path = "/Users/dev/Documents/introvert logo/introvert_icon_fano_transparent.png"

with open(logo_path, "rb") as f:
    logo_base64 = base64.b64encode(f.read()).decode("utf-8")

with open(icon_path, "rb") as f:
    icon_base64 = base64.b64encode(f.read()).decode("utf-8")

with open(html_path, "r") as f:
    html_content = f.read()

# Replace the logo-container inner content
target_content = """        <div class="logo-container">
            <!-- vector SVG Introvert Icon -->
            <svg class="logo-icon" viewBox="0 0 32 32">
                <rect x="4" y="4" width="24" height="24" rx="6" stroke="url(#cyber-grad)" stroke-width="2" fill="none" />
                <circle cx="16" cy="12" r="3" />
                <path d="M10 22 C 10 18, 22 18, 22 22" stroke-width="2" stroke-linecap="round" />
            </svg>
            <span class="logo-text">INTROVERT RBN</span>
        </div>"""

replacement_content = f"""        <div class="logo-container" style="display: flex; align-items: center; gap: 0.5rem;">
            <img src="data:image/png;base64,{icon_base64}" class="logo-icon" style="width: 36px; height: 36px; object-fit: contain;" />
            <img src="data:image/png;base64,{logo_base64}" class="logo-text-img" style="height: 24px; object-fit: contain; margin-left: 0.5rem;" />
            <span class="logo-text" style="font-size: 0.9rem; font-weight: 600; color: var(--text-muted); margin-left: 0.5rem; border-left: 1px solid var(--tile-border); padding-left: 0.5rem; letter-spacing: 0.05em; font-family: 'Inter', sans-serif;">RBN OPERATOR</span>
        </div>"""

new_html = html_content.replace(target_content, replacement_content)

with open(html_path, "w") as f:
    f.write(new_html)

print("Successfully embedded base64 assets in src/dashboard.html")
