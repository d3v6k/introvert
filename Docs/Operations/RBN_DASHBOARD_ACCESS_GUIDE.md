# RBN Operator Dashboard Access Guide

This guide explains how to securely access the Web GUI dashboard of your Root Bootstrap Node (RBN) from your local computer (Windows, macOS, or Linux).

To ensure your operator wallet private key is kept safe, the dashboard web server is bound only to **localhost** (`127.0.0.1:8080`). It is not exposed to the public internet.

---

## 💻 Case 1: Node is Running on Your Local Desktop
If you are running the RBN node directly on the computer you are currently using:
1. Open any web browser.
2. Go to: **[http://localhost:8080](http://localhost:8080)**
3. The dashboard will load immediately. No tunnels are required!

---

## ☁️ Case 2: Node is Running on a Remote VPS
If your RBN node is running on a remote cloud server (Alibaba Cloud, AWS, DigitalOcean, Hetzner, etc.), you must establish a secure encrypted tunnel (SSH Port Forwarding) to map the remote dashboard port to your local machine.

Select the instructions for your operating system below:

### 🍎 macOS & 🐧 Linux (Terminal)
1. Open your **Terminal** application.
2. Run the following port-forwarding command (replace `YOUR_VPS_IP` with your actual server IP):
   ```bash
   ssh -N -L 8080:localhost:8080 root@YOUR_VPS_IP
   ```
   *(If you configured a non-root user, replace `root` with your username).*
3. Keep this terminal window open.
4. Open your web browser and go to: **[http://localhost:8080](http://localhost:8080)**

---

### 🪟 Windows 10 & 11 (PowerShell / Command Prompt)
Modern Windows versions include a native OpenSSH client.
1. Press the **Windows Key**, type `powershell` or `cmd`, and press Enter.
2. Run the port-forwarding command:
   ```cmd
   ssh -N -L 8080:localhost:8080 root@YOUR_VPS_IP
   ```
3. Keep this console window open.
4. Open your browser and go to: **[http://localhost:8080](http://localhost:8080)**

---

### 🪟 Windows (Using PuTTY GUI)
If you prefer a graphical interface:
1. Open **PuTTY**.
2. In the **Session** category, enter your VPS Host Name or IP address.
3. In the left tree, navigate to: **Connection ➔ SSH ➔ Tunnels**.
4. Configure the tunnel:
   *   **Source port:** `8080`
   *   **Destination:** `localhost:8080`
   *   Select **Local** and **Auto**.
5. Click the **Add** button. You should see `L8080 localhost:8080` appear in the list.
6. Click **Open** at the bottom to connect to your VPS and log in.
7. Open your browser and go to: **[http://localhost:8080](http://localhost:8080)**

---

## 🔒 Security Best Practices
*   **Keep Port 8080 Firewalled:** Never open port `8080` in your VPS security group or firewall (e.g. `ufw`). Only port `443` (for libp2p P2P traffic) and port `22` (for SSH) should be open to the public.
*   **Close Tunnels When Done:** When you are finished monitoring your node or exporting keys, close your tunnel session (by pressing `Ctrl + C` in your terminal or closing PuTTY).
