# Roadmap

culvert-async moves in lockstep with culvert: it uses culvert's register map
(`culvert::codec`) and gains the async counterpart of each new culvert method in its next
release. Its register items are therefore culvert's — see culvert's
[`doc/roadmap.md`](https://github.com/ashlar-display/culvert/blob/main/doc/roadmap.md):

- **`LIP_Update` (`Update_0` bit 7, HDMI 2.2)** — a field for the flag.
- **`Update_1` (0x11)** — wrapped if a field is ever defined.
- **DSC status** — the DSC status registers.
- **`Test_Config_0` (0xC0)** — the read-request test register, with read request support.
- **Manufacturer identification (0xD0–0xDD)** — vendor-defined identification data.

And culvert's additions to registers it already reads, each additive:

- **One CED and RS read (0x50–0x5A)** — both in one transaction, for a link health monitor;
  designed with the monitor.
- **Read-back decoders for register dumps** — `Config_0`, `Config_1`.
- **Raw CED bytes in `CedReading`** — when a consumer needs them.

None of these will be added here ahead of culvert.

## culvert-async's own

- **`SendScdc<T>`** — a second client type for code bounded by a `Send` twin, which takes
  no `Scdc<T>` (see `doc/architecture.md`, "`Send` futures"). One wrapper serves both
  consumers' twins: plumbob-async's `SendScdcClient`, implemented here with the
  `plumbob-async` feature, and spiritlevel-async's `SendTmdsScdc`, implemented in
  spiritlevel-async as its `TmdsScdc` for `Scdc<T>` is. So code generic over the twins trains
  FRL and brings up TMDS on one client; a wrapper defined by each consumer would give a
  caller two types for one client, of which no value implements both twins. Its shape is
  chosen with both consumers in mind: how the client is reached and taken back (a public
  field or accessors), and whether a borrow, `&mut SendScdc<T>`, implements the twins too,
  so that a client can be lent to each procedure in turn as `&mut Scdc<T>` is. Its trigger,
  a consumer generic over `SendScdcClient`, is now in sight: the planned link
  orchestrator's async companion drives training generic over the SCDC client, so that a
  third-party client can be used. Additive; it follows plumbob-async's move to
  hdmi-hal-async's `FrlPhy`.
- **`DdcError` re-exports** — `DdcError`, `DdcErrorKind` and `NoAcknowledgeSource` next to
  the transport traits, since an implementor of `ScdcTransport` writes them; additive, with
  culvert's.
