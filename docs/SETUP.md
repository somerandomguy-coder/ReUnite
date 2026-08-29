# Setup: running the mesh across several computers

This guide takes you from a bare laptop to a working multi-machine mesh. It assumes
nothing is installed. Everything here works with **no internet connection** — the only
step that needs internet is installing the toolchain and building, which you do
beforehand.

Contents:

1. [Install the toolchain](#1-install-the-toolchain-once-per-machine)
2. [Get the code and build it](#2-get-the-code-and-build-it)
3. [Put the laptops on one network](#3-put-the-laptops-on-one-network)
4. [Open the firewall](#4-open-the-firewall)
5. [Run it](#5-run-it)
6. [Verify the mesh](#6-verify-the-mesh)
7. [If discovery does not work: --peer](#7-if-discovery-does-not-work-peer)
8. [Several nodes on one laptop](#8-several-nodes-on-one-laptop)
9. [Where your data lives](#9-where-your-data-lives)
10. [Troubleshooting](#10-troubleshooting)

---

## 1. Install the toolchain (once per machine)

You need Rust 1.75 or newer (built and tested on 1.98).

**macOS / Linux**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version
```

macOS will also need the Apple command line tools if you have never compiled anything:
`xcode-select --install`.

**Windows**

Download and run <https://win.rustup.rs> (`rustup-init.exe`), accept the defaults, and
install the "Desktop development with C++" Build Tools when it offers. Then, in a new
PowerShell window:

```powershell
rustc --version
```

**Linux** also needs a linker: `sudo apt install build-essential` (Debian/Ubuntu) or
`sudo dnf install gcc` (Fedora).

## 2. Get the code and build it

Do this **while you still have internet** — the build downloads crates. Repeat it on
every laptop, or build once and copy the resulting binary to machines with the same OS
and CPU architecture.

```bash
git clone <this-repo>
cd UniHack2026
cargo build --release
```

The binary lands at:

* macOS / Linux — `./target/release/meshnet`
* Windows — `.\target\release\meshnet.exe`

Optional: `cargo install --path crates/meshcli` puts `meshnet` on your `PATH`.

Check it runs:

```bash
./target/release/meshnet --help
```

**No internet on the other laptops?** Build on one machine and copy the single
`meshnet` binary over (USB stick, AirDrop, `scp`). It is self-contained. A binary built
on Apple Silicon macOS does not run on Windows or on Intel macOS — build one per
platform, or install Rust on each.

## 3. Put the laptops on one network

The nodes talk over **UDP on port 47474**, using multicast `239.42.13.7` and subnet
broadcast for discovery. They need a shared layer-2 network — **but that network does not
need internet, a gateway, or DNS.** Pick whichever of these you can get:

| Situation | What to do |
| :--- | :--- |
| A Wi-Fi router is still powered (even with the internet down) | Join every laptop to the same SSID. Done. |
| No router, but someone has a phone | Turn on the phone's hotspot and join every laptop to it. Mobile data can be off. |
| No router, no phone, macOS host | System Settings → General → Sharing → **Internet Sharing** → share from any interface **to Wi-Fi**, set a network name and WPA2 password, turn it on. Others join that SSID. |
| No router, no phone, Windows host | Settings → Network & Internet → **Mobile hotspot** → Edit the name/password → turn it on. Others join that SSID. |
| No router, no phone, Linux host | `nmcli device wifi hotspot ifname wlan0 ssid meshnet password meshnet123` |
| Wired | An Ethernet switch, or two laptops with a single Ethernet cable (modern ports auto-crossover; both get 169.254.x.x link-local addresses, which is fine). |

Confirm they are on the same subnet — the first three numbers of the IPv4 address should
match:

```bash
ipconfig getifaddr en0     # macOS Wi-Fi
hostname -I                # Linux
ipconfig                   # Windows: look at "IPv4 Address"
```

Then check they can reach each other: `ping 192.168.x.y` from one to the other. If ping
fails, mesh traffic will not flow either — fix the network first (usually the firewall,
step 4, or "client isolation"/"AP isolation" on a guest Wi-Fi network, which blocks
laptop-to-laptop traffic entirely; use a hotspot instead).

## 4. Open the firewall

Incoming UDP on port 47474 must be allowed.

**macOS** — first check whether the firewall is even on. On many Macs it is off, and
then there is nothing to do:

```bash
/usr/libexec/ApplicationFirewall/socketfilterfw --getglobalstate
```

* `Firewall is disabled. (State = 0)` — nothing to configure, incoming UDP already works.
* `Firewall is enabled.` — `meshnet` needs to be allowed, see below.

**Do not count on a permission pop-up.** *"Do you want the application meshnet to accept
incoming network connections?"* appears only when the firewall is enabled, and often not
even then: if *Settings → Network → Firewall → Options → Automatically allow downloaded
signed software* is ticked, macOS silently adds Cargo-built binaries to the allow list
instead of asking. Click **Allow** if you do get it, but verify rather than wait for it:

```bash
# is meshnet allowed?
/usr/libexec/ApplicationFirewall/socketfilterfw --listapps | grep -A2 meshnet

# allow it explicitly (only needed if the firewall is enabled)
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add "$(pwd)/target/release/meshnet"
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp "$(pwd)/target/release/meshnet"
```

One wrinkle if the firewall is enabled: every `cargo build` re-signs the binary with a
fresh ad-hoc signature, so macOS may treat the rebuilt `meshnet` as a new application and
need the decision again. Re-run the two commands above after rebuilding, or just copy the
release binary somewhere stable and run it from there.

**Windows** — the first run shows a Windows Defender Firewall dialog. Tick **Private
networks** and click **Allow access**. Or, in an Administrator PowerShell:

```powershell
netsh advfirewall firewall add rule name="meshnet" dir=in action=allow protocol=UDP localport=47474
```

Windows also blocks all traffic on networks marked "Public". Set the hotspot/Wi-Fi to
**Private**: Settings → Network & Internet → Wi-Fi → your network → Network profile type
→ Private.

**Linux**

```bash
sudo ufw allow 47474/udp                       # ufw
sudo firewall-cmd --add-port=47474/udp         # firewalld
```

## 5. Run it

On each laptop:

```bash
./target/release/meshnet --name alice        # use a different name on each machine
```

You land in the public `[default]` network straight away:

```
offline mesh node started - you are in [default]
  node id  : b809ed72839ec44b
  radios   : udp/0.0.0.0:47474, multicast 239.42.13.7:47474, broadcast
  not used - bluetooth: this OS cannot advertise as a BLE peripheral - run scripts/ble_gateway.py to bridge one
  home     : /Users/alice/.meshnet
  type --help for commands, or just type a message to broadcast
[default] >
```

**`meshnet` starts every radio the machine has**, and meshes over all of them at once.
A radio that cannot start is named with the reason and skipped — it costs that radio, not
the node. On Linux that line lists Bluetooth alongside Wi-Fi; on macOS and Windows it does
not, because neither can advertise as a BLE peripheral from userspace (deviation D3).

Useful flags:

| Flag | Use it when |
| :--- | :--- |
| `--name alice` | Give peers a readable name (they can still re-label you locally) |
| `--lat 10.7769 --lon 106.7009` | Start with a GPS position (there is no GPS receiver on a laptop; set it by hand or with `--set-location`) |
| `--port 47475` | Another program owns 47474, or you are running a second node on one machine |
| `--peer 192.168.1.42:47474` | Multicast is blocked — see step 7 |
| `--home ./nodeA` | Keep this node's identity and messages in a specific folder |
| `--transport udp` / `--transport ble` | Pin one radio instead of using all of them. For testing; the default is `all` |

### Joining on boot

So that "switched on" really is the whole onboarding:

```bash
./scripts/autostart/install.sh            # launchd (macOS) or systemd --user (Linux)
./scripts/autostart/install.sh --remove   # undo
```

It runs `meshnet` with no arguments — already zero-config — and restarts it if it dies.
Logs go to `~/.meshnet/meshnet.log` on macOS, and `journalctl --user -u meshnet -f` on
Linux. To keep meshing while logged out on Linux: `sudo loginctl enable-linger $USER`.

On Windows, put a shortcut to `target\release\meshnet.exe` in
`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`.

### The node goes quiet when it is alone

After about a minute with no peers the beacon slows from every 3 seconds to every 10, then
30, then 60 — and the log says so once per change. It snaps back to 3 seconds the moment
anything is heard, and never backs off while an SOS is active. This is deliberate; see
[ARCHITECTURE.md](ARCHITECTURE.md#the-duty-cycle).

## 6. Verify the mesh

Within a few seconds each node prints `+ peer ~bob (d94753fd) is in range`. Then:

```
[default] > --peers
ID                 NAME             LINK    HOPS   RTT      DISTANCE   SEEN    NET
d94753fd0112feca   ~bob             direct  1      14ms     345m       1s      yes
16ff2f7c934f4ac9   ~carol           relayed 2      -        1.46km     1s      yes
```

* `direct` = you hear that node's radio yourself; `relayed` = you reach it through others.
* `HOPS` = length of the route. `RTT` = measured round-trip latency to a direct neighbour.
* `DISTANCE` = GPS distance, once both sides have set a position.
* The list is sorted **nearest first**, which is what a rescue coordinator wants to see.

Now try, on laptop A:

```
[default] > hello, is anyone receiving this?
[default] > --set-location 10.7769 106.7009
[default] > --share-location
[default] > --rename d94753fd0112feca bob-medic
```

...and watch it appear on the others. For the full walkthrough — private networks,
multi-hop relaying, kick voting — follow [DEMO.md](DEMO.md).

To leave, type `--quit` (or press Ctrl-C).

## 7. If discovery does not work: `--peer`

Some networks (campus Wi-Fi, hotel Wi-Fi, some corporate APs, and VPN interfaces) silently
drop multicast and broadcast. Unicast still works, so point the nodes at each other by
hand. You only need to seed **one** direction and **one** pair — the mesh gossips the rest,
and the transport keeps talking to any address it has heard from.

On laptop A, find the address:

```bash
ipconfig getifaddr en0        # e.g. 192.168.1.41
```

Start A normally, then start B with:

```bash
./target/release/meshnet --name bob --peer 192.168.1.41:47474
```

`--peer` may be repeated. If discovery frames are being echoed back to you or the network
is noisy, you can also turn the flood off entirely and drive the topology by hand:

```bash
./target/release/meshnet --no-multicast --no-broadcast --peer 192.168.1.41:47474
```

## 8. Several nodes on one laptop

Useful for testing before the other people arrive. Give every node **its own home
directory and its own port**, and seed them at each other:

```bash
# terminal 1
./target/release/meshnet --home ./nodeA --port 47001 --name alice --no-multicast --no-broadcast

# terminal 2
./target/release/meshnet --home ./nodeB --port 47002 --name bob   --no-multicast --no-broadcast \
    --peer 127.0.0.1:47001

# terminal 3 — only hears B, so anything to A must be relayed by B
./target/release/meshnet --home ./nodeC --port 47003 --name carol --no-multicast --no-broadcast \
    --peer 127.0.0.1:47002
```

That last arrangement gives you a real A—B—C line topology on one machine, which is how
[DEMO.md](DEMO.md) demonstrates multi-hop relaying without walking three laptops apart.

Two rules for multiple nodes on one machine:

* **A different `--home` is mandatory.** The identity lives in the home directory, so two
  processes sharing one are *the same node*: they have the same node id, discard each
  other's frames as their own echo, and never appear in each other's `--peers`. Since
  v2 the node detects this and prints `another process is running this same identity`.
* **A different `--port` is strongly recommended.** Two nodes *can* share port 47474 and
  still find each other over multicast loopback, but a unicast datagram to a shared port
  is delivered to only one of the sockets, so routed direct messages become a coin flip.

**Do not mix styles.** Every node in one experiment must be reachable by the others:

```bash
# BROKEN: these two can never meet
./target/release/meshnet                                      # port 47474, multicast on
./target/release/meshnet --home ./nodeB --port 47002 \
    --no-multicast --no-broadcast --peer 127.0.0.1:47001      # only ever talks to :47001

# the first listens on 47474; the second speaks only to 47001, which nobody is on.
```

Either let every node discover by multicast (plain flags, just vary `--home`), or turn
discovery off on all of them and wire the `--peer` addresses to ports that actually exist,
as in the three-node block above.

## 9. Where your data lives

Everything is under one directory: `~/.meshnet` by default (`%USERPROFILE%\.meshnet` on
Windows), or whatever you pass to `--home`, or `$MESHNET_HOME`.

| File | Contents |
| :--- | :--- |
| `identity.json` | Your UUID, node id and **private keys** |
| `contacts.json` | Public keys, aliases and last known GPS of peers you have met |
| `networks.json` | Private networks you belong to, **including their symmetric keys** |
| `messages/<network-id>.jsonl` | Message log, written only for networks with `--enable-storing` |

Two consequences worth knowing:

* Delete the home directory and you become a **new node** with a new id, and you lose
  access to every private network you were invited to.
* Anyone with read access to that folder can read your networks. It is not encrypted at
  rest in this MVP.

## 10. Troubleshooting

| Symptom | Cause and fix |
| :--- | :--- |
| `--peers` is empty on every machine | Different Wi-Fi networks, or the firewall (step 4). Check `ping` works between the laptops first. |
| `ping` works but `--peers` is still empty | Multicast/broadcast is blocked. Use `--peer` (step 7). |
| `could not bind UDP port 47474` | Another node is already running on this machine — use `--port 47475`, or quit the other one. |
| Two nodes on one machine, both `--peers` empty | Either they share a `--home` (they are one node — the client now warns about this), or their ports/`--peer` addresses do not line up. See step 8. |
| `ignoring frames from ...: unsupported protocol version` | One machine is running an older build. Rebuild them all from the same commit and restart every node. |
| `another process is running this same identity` | Two nodes are sharing one home directory. Give the second one `--home ./nodeB --port 47475`. |
| Peers appear, then `- peer ... went quiet` | The node stopped hearing beacons for 30s: the machine slept, the Wi-Fi dropped, or it moved out of range. It comes back on its own. |
| Guest/campus Wi-Fi, nothing works | "Client isolation" blocks laptop-to-laptop traffic at the access point. Use a phone hotspot instead. |
| A message says `no route yet - flooding and retrying` | Normal. The message is queued and retried for 2 minutes; you get `✓ delivered` when it lands. |
| `no public key for ... yet` when inviting | You have not heard that node's beacon yet. Wait for them to show in `--peers`. |
| Windows: nothing arrives even with the rule added | The network is marked "Public". Set the profile to Private (step 4). |
| macOS: no permission prompt appeared | Expected in most cases — the prompt only shows when the firewall is enabled, and is skipped entirely when "automatically allow signed software" is on. Confirm with `socketfilterfw --getglobalstate` and `--listapps` (step 4) instead of waiting for it. Nothing is wrong if peers show up in `--peers`. |
| Text you type gets overwritten by incoming messages | Cosmetic. The MVP prints events over the prompt line; press Enter and retype. |
