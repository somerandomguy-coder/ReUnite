# Joining the mesh

**For anyone using ReUnite.** No terminal, no setup, no account.

If you are setting this up for other people, or building it, read
[SETUP.md](SETUP.md) and [MOBILE.md](MOBILE.md) instead.

---

## 1. Switch the device on

That is the whole thing.

Open ReUnite. It creates your identity the first time, joins the shared `[default]`
network, and starts talking on every radio your device has. There is no sign-up, no
password, no phone number, and it never touches the internet.

You do not choose a radio. The app uses all of them.

---

## 2. What it can reach, and how far

| Radio | Reaches | Roughly how far | Needs |
| :--- | :--- | :--- | :--- |
| **Bluetooth** | phones near you | 10–30 m outdoors, much less through walls | nothing at all |
| **Wi-Fi** | anything on the same Wi-Fi or hotspot | the whole network | a shared Wi-Fi or hotspot — **no internet required** |

Phones use both. Laptops use Wi-Fi (and Bluetooth too, on Linux).

**Distance is not the limit — people are.** Messages hop from device to device. If your
friend is too far away but somebody else is standing between you, the message goes
through them automatically. Nobody has to agree to relay, and the person in the middle
cannot read a private network's traffic while passing it on.

---

## 3. What you should see

Within a few seconds of another device coming into range, it appears on your **Peers**
screen, nearest first.

If nothing appears, the **Networks** tab shows why in plain words:

| It says | It means |
| :--- | :--- |
| `Bluetooth radio is on`, `Connected peers 0` | Working. Nobody is in range yet. |
| `Bluetooth is switched off` | Switch it on. Wi-Fi still works meanwhile. |
| `Bluetooth permission was refused` | Allow it in your phone's settings for ReUnite. |
| `No Bluetooth mesh on this platform` | A Mac or Windows laptop. It meshes over Wi-Fi. |
| `Waiting for the radio to report in` | Give it a second. |

**"No peers heard yet" is not an error.** It means the radio is listening and nobody is
there. Someone walking into range appears on their own, with no action from you.

---

## 4. When there is no Wi-Fi at all

Two phones need nothing. Bluetooth alone is enough — keep both apps **open and on the
screen** and get within a few metres for the first connection.

To bring laptops in as well, make a network without internet:

* **Any phone:** turn on the personal hotspot. Everyone joins it. It will say "no
  internet" — that is fine, and it is the point.
* **A laptop:** create a hotspot from its network settings.

Everyone on that hotspot is on the mesh, whether or not it reaches the outside world.

---

## 5. The one thing to know about SOS

The SOS in this app alerts **only the people on this mesh**. It does not call emergency
services, an ambulance, or anyone outside Bluetooth or Wi-Fi range. It never will — that
is deliberate, so that testing the app can never dial a real emergency line.

If you need real emergency services and have any signal at all, use your phone's own
emergency call.

---

## 6. Battery

The app slows down when it is alone. After about a minute with nobody around it beacons
less often, and after twenty minutes less often still — so a phone left on overnight is
still findable in the morning instead of flat by 2 a.m.

It speeds straight back up the moment anything is heard, and **it never slows down while
an SOS is active**, yours or anyone else's.

You do not have to manage any of this.

---

## 7. If something is wrong

| Problem | Try |
| :--- | :--- |
| No peers, ever | Both devices need the app open. On phones, keep it on screen — see below. |
| Peers appear, messages do not | Check you are both in the same network on the **Networks** tab (`[default]` unless you changed it). |
| It works, then stops when the screen locks | Known limit today: the mesh pauses when the app is backgrounded. Keep it on screen. |
| An iPhone is invisible to an Android phone | Keep the iPhone's app on screen. Backgrounded iPhones hide from non-Apple devices — an Apple restriction, not something this app can fix. |
| Someone vanished from Peers | They stay as a grey **ghost** at the last place they were seen, with how long ago. That is usually a flat battery, not a person who moved. |
