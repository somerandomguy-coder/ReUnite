# Demo script

A ten-minute walkthrough that shows every Phase 1 feature: discovery, multi-hop relaying,
private networks that relays cannot read, kick voting with an automatic re-key, and
store-and-forward delivery. Every transcript below is real output from the built binary.

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
```

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
