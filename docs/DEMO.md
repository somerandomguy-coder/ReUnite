# Demo script

A ten-minute walkthrough that shows every Phase 1 feature: discovery, multi-hop relaying,
private networks that relays cannot read, kick voting with an automatic re-key,
store-and-forward delivery, the in-network SOS, pre-canned panic messages, last-known-location
ghosting and the aggregated safe-zone heat map. Every transcript below is real output from
the built binary.

> **Run the same build on every machine.** The protocol version is 3; a node on an older
> build is rejected with a clear version-mismatch message rather than misbehaving.

You need three nodes — three laptops, or three terminals on one machine. If you are using
one machine, start them as in [SETUP.md §8](SETUP.md#8-several-nodes-on-one-laptop), which
gives you an **A—B—C line**: A and C cannot hear each other, so B must relay between them.
On three real laptops, either walk C out of A's Wi-Fi range, or fake it with `--isolate`
(step 2 below).

Throughout: `$A`, `$B`, `$C` are the node ids each client prints at startup.

---

## 1. Everyone lands in `[default]`

Each node starts in the public lobby and beacons its presence.

```
[default] > --whoami
node id  : b809ed72839ec44b
name     : alice
network  : [default]
transport: udp/0.0.0.0:47001
home     : /Users/alice/.meshnet
location : (not set - use --set-location)
battery  : 88%
status   : (none)
zones    : H3 resolution 8
```

Battery is read from the platform. Pass `--battery 42` to force a value, which is what
keeps a demo reproducible and gives a mains-powered desktop something to advertise.

Give them positions (a laptop has no GPS, so we set it by hand; on mobile this comes from
the OS) and look around from **A**:

```
[default] > --set-location 10.7769 106.7009
[default] > --peers
ID                 NAME             LINK    HOPS   RTT      DISTANCE   SEEN    NET
d94753fd0112feca   ~bob             direct  1      14ms     345m       1s      yes
16ff2f7c934f4ac9   ~carol           relayed 2      -        1.46km     1s      yes
sorted nearest first (GPS distance, then hops, then latency)
```

`~bob` is a name Bob advertises; the `~` marks it as unverified. Pin down what *you* want
to see:

```
[default] > --rename d94753fd0112feca bob-medic
d94753fd0112feca will show as 'bob-medic' on this device
```

That alias is local — it never leaves the laptop, which is the point of `--rename` in
[plan.md](../plan.md) step 1.2.

## 2. Multi-hop relaying

Note the table above: `~carol` is `relayed`, `2` hops. A cannot hear C at all. Confirm the
route A learned:

```
[default] > --routes
DEST               NAME             NEXT HOP           HOPS   AGE
d94753fd0112feca   bob-medic        d94753fd0112feca   1      now
16ff2f7c934f4ac9   ~carol           d94753fd0112feca   2      now
```

Now broadcast from **A**:

```
[default] > this is alice, we need water at block 4
```

**C** receives it, and reports that it arrived two hops away:

```
07:21:47 [default] ~alice: this is alice, we need water at block 4 (2h)
```

B relayed it without being asked. That is the mesh doing its job.

> **On three laptops in one room** every node hears every other one directly, so nothing
> gets relayed and the demo looks trivial. Force a line topology with `--isolate`, which
> makes a node deaf to everyone except the listed ids:
>
> ```
> on A:  --isolate $B          # A only hears B
> on C:  --isolate $B          # C only hears B
> ```
>
> A and C are now two hops apart while sitting next to each other. `--isolate` with no
> arguments clears it.

## 3. A private network the relay cannot read

Everything above was in `[default]`, which is public by design. Now **A** makes a private
network and invites **C** — the node that is only reachable *through B*.

```
[default] > --create-network rescue
created private network 'rescue' (9e18e88797b864ba) and switched to it - invite people with --network rescue --add [id]
[rescue] > --network rescue --add 16ff2f7c934f4ac9
sealed the 'rescue' key to ~carol and sent the invite
```

The prompt changed to `[rescue]`. On **C**:

```
! ~alice added you to network 'rescue' (2 members). Use --switch rescue to talk there.
[default] > --switch rescue
```

Send a private message from **A** to **C**:

```
[rescue] > --msg 16ff2f7c934f4ac9 meet at the school roof
[rescue] -> ~carol: via bob-medic (2 hops)
✓ delivered to ~carol: "meet at the school roof"
```

**C** gets it:

```
07:21:59 [rescue] (direct) ~alice: meet at the school roof (2h)
```

**B carried that message and cannot read a word of it.** Check B's view:

```
[default] > --networks
    NAME               ID                 MEMBERS  EPOCH   STORING
*   default            -                  2        0       off
```

B knows nothing about `rescue`. The network key was sealed to Carol's X25519 public key
and never travelled in the clear — [plan.md](../plan.md) step 1.3.

## 4. Keeping a copy on disk

Storage is off by default. Turn it on per network, on each device that wants a copy:

```
[rescue] > --network rescue --enable-storing
message storage for 'rescue' is now ON (/Users/alice/.meshnet/messages/9e18e88797b864ba.jsonl)
[rescue] > --history
07:21:59 [rescue] b809ed72->16ff2f7c meet at the school roof
```

The file is plain JSON lines, one message per line, ready for a map view to consume:

```json
{"ts_ms":1787988119576,"network":"9e18e88797b864ba","network_name":"rescue","kind":"direct",
 "from":"b809ed72839ec44b","to":"16ff2f7c934f4ac9","text":"meet at the school roof"}
```

## 5. Voting someone out

Add **B** to `rescue` so there are three members, then have two of them vote him out.
Threshold is `>= half the members`, so 3 members need 2 votes.

On **A**:

```
[rescue] > --network rescue --add $B
[rescue] > --kick $B
[rescue] vote to kick ~bob cast (needs 2)
! [rescue] alice voted to kick ~bob (1/2)
```

On **C** (`--switch rescue` first):

```
[rescue] > --kick $B
! [rescue] carol voted to kick ~bob (2/2)
! [rescue] ~bob removed; network re-keyed to epoch 1
```

The moment the second ballot lands, the lowest-id remaining member mints a **fresh network
key** and seals it to everyone still in — deterministically, with no server and no
designated host. **A** sees:

```
! network 'rescue' re-keyed to epoch 1 (2 members)
[rescue] > --networks
    NAME               ID                 MEMBERS  EPOCH   STORING
*   rescue             9e18e88797b864ba   2        1       on
      members: ~carol, alice
```

Now prove it. On **A**:

```
[rescue] > bob must NOT see this line
```

**C** receives it. **B**, who is still relaying the packet, sees nothing — his copy of the
key is one generation old and no longer opens anything:

```
[rescue] > --networks
*   rescue             9e18e88797b864ba   3        0       off
```

B still believes he is a member at epoch 0. Nobody tells a removed node it was removed;
it simply stops being able to decrypt. [plan.md](../plan.md) step 1.4.

## 6. Store-and-forward when the relay disappears

Quit **B** — the only path between A and C — then send from **A**:

```
[rescue] > --msg $C waiting for a relay
```

Nothing is lost. The message sits in A's outbox and is retried. Start **B** again and,
within about 15 seconds:

```
✓ delivered to ~carol: "waiting for a relay"
```

C receives it as if nothing had happened. Undeliverable messages are retried for two
minutes before A reports giving up. [plan.md](../plan.md) step 1.5.

## 7. Sharing position

```
[rescue] > --set-location 10.7769 106.7009
[rescue] > --share-location
[rescue] shared 10.77690, 106.70090
```

Everyone in the network sees it, with the distance from their own position:

```
@ ~alice is at 10.77690, 106.70090 (1.46km away)
```

`--peers` then ranks people nearest-first, which is the input a mapping UI needs in
Phase 2.


## 8. Pre-canned panic messages

A frightened person should not have to type, and the radio budget does not stretch to
prose. Seven common things travel as **one byte**:

```
[default] > --status
pre-canned status messages - one byte on the wire
  1  safe       I am safe
  2  medical    Need medical help
  3  supplies   Need water / food
  4  trapped    Trapped - need rescue
  5  moving     Moving to a safe zone
  6  shelter    Shelter here, space available
  7  hazard     Route blocked / hazard
  0  none       clear your status
usage: --status medical   (or --status 2)

[default] > --status medical
[default] status: Need medical help (1 byte, code 2)
```

Everyone else sees the words, reconstructed locally:

```
* ~bob: Need medical help
```

The status also rides every `Hello`, so a node that arrives ten minutes later still learns
it instead of having missed the one broadcast.

## 9. In-network SOS

On **C**, who can only be heard through relay **B**:

```
[default] > --sos start
[default] SOS broadcast to the mesh (ttl 12). This alerts nearby nodes only - it does not
call emergency services. --sos stop to clear.
```

On **A**, two hops away:

```
!! SOS from ~carol at 10.77690, 106.70090 - mesh alert only, emergency services were NOT called

[default] > --peers
ID                 NAME             LINK     HOPS   RTT      DISTANCE   BATT   SEEN    NET
5fd15953d9186071   ~relay           direct   1      1ms      -          50%    1s      yes
acfd53bb3f4e5430   ~carol           relayed  2      -        -          7%     now     yes
  !! SOS - last heard just now
```

SOS is sent at TTL 12 instead of the usual 8 and does not wait for the next beacon — it is
the one packet class allowed to be noisy. The dedupe cache is what stops that becoming a
storm.

**It is not the operating system's SOS.** [plan.md](../plan.md) §3.2 isolates the two
deliberately, so that testing a mesh can never dial real emergency services.

## 10. Ghosting: a dead battery is not a deletion

Quit **B** entirely and wait past the 30-second neighbour timeout. B does not disappear
from A's map:

```
[default] > --peers
ID                 NAME             LINK     HOPS   RTT      DISTANCE   BATT   SEEN    NET
d965fdd41a1a1940   ~bob             ghost    -      -        1.11km     3%     38s     yes
  last seen at 10.78690, 106.70090 38s ago
  * I am safe
sorted nearest first (GPS distance, then hops, then latency); ghosts last
1 ghost(s): unreachable now, showing their last known position
```

Ghosts sort below every reachable peer, keep their last GPS fix, and carry the age of that
fix. Where someone was last seen is exactly what a search needs.

## 11. Safe and unsafe zones

Raw coordinates would flood the network, so a report is snapped to an H3 hex cell
(resolution 8, about a town block) and only the cell travels — carrying the verdict and
the radius the reporter chose. On **B**:

```
[default] > --report-zone 10.7769 106.7009 unsafe 750 m
[default] reported unsafe within 750 m of cell 8865b5662bfffff - now reads unsafe (0 safe / 1 unsafe)
```

The unit is optional and defaults to metres; `km`, `ft` and `mi` all work, so
`--report-zone 10.7769 106.7009 safe 0.5 km` and `... safe 1640 ft` are the same claim.

On **C**, a few metres away — the same cell, a different opinion:

```
[default] > --report-zone 10.77695 106.70095 safe 300 m
```

On **A**:

```
# zone 8865b5662bfffff now reads unsafe within 750 m (1 safe / 1 unsafe, via ~carol)

[default] > --heatmap show
CELL               LAT          LON          VERDICT   RADIUS    SAFE    UNSAFE  AGE    MINE
8865b5662bfffff    10.77508     106.69941    unsafe    750 m     1       1       2s
a cell is safe only when more people vouch for it than against it - a tie reads unsafe.
radius is the mean of the reports that agree with the verdict.
```

Three things to notice.

**A tie reads unsafe.** One person says safe, one says unsafe, and the cell renders red.
There is no amber, and no averaging into "moderate" — a contested area is not a safe area,
and the false alarm is the cheaper mistake. Have a third node report `safe` and the cell
flips to safe with `2 safe / 1 unsafe`; the dissent stays on screen either way.

**The radius is the mean of the reports that agree with the verdict.** While the cell reads
unsafe it shows B's 750 m, not the average of 750 and 300 — C was describing a different
claim about the same ground.

**Consensus counts people, not reports.** Have C report the same cell again and the count
stays at 2 — a node re-reporting replaces its own earlier opinion and can never manufacture
agreement. A cell with only one report is printed `1 (unverified)`, because one person
calling a street safe is not the same claim as thirty.

**Late joiners converge.** Each node re-gossips one of its own reports per maintenance
tick, so a node that starts up after the reports were made still ends up with the whole
map. Start a fourth node now and watch its heat map fill in over the next few seconds.
Reports expire after six hours: a node that has gone away stops refreshing and its opinion
correctly ages out.
