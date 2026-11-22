//! The SCDC client over hdmi-hal-async's `SendScdcOverDdc`, against maquette's simulated
//! sink on its async DDC bus.

use core::time::Duration;

use culvert_async::{ClearableUpdateFlags, ScdcError, TmdsConfig};
use maquette::hdmi_hal_async::phy::{SendPhyPower, SendTmdsPhy};
use maquette::hdmi_hal_async::scdc::SendScdcOverDdc;
use maquette::hdmi_hal_async::time::SendDelay;
use maquette::{
    Bench, BusError, ColorBitDepth, ColorFormat, Ddc, DdcError, DdcErrorKind, Effect, Fault,
    Filter, NoAcknowledgeSource, Phy, SinkConfig, Time, TmdsClockRatio, TmdsPhyConfig, TmdsVideo,
};

type Client<'b> = culvert_async::Scdc<SendScdcOverDdc<Ddc<'b>>>;

fn client<'b>(bench: &'b Bench<'_>) -> Client<'b> {
    culvert_async::Scdc::new(
        SendScdcOverDdc::new(bench.ddc()).expect("the bench's bus serves SCDC"),
    )
}

fn high() -> TmdsPhyConfig {
    let video = TmdsVideo::new(594_000, ColorBitDepth::Depth8, ColorFormat::Rgb444);
    TmdsPhyConfig::new(594_000, TmdsClockRatio::Div40, video)
}

/// The kind of a transport error: `None` for a protocol error.
fn kind(error: &ScdcError<BusError>) -> Option<DdcErrorKind> {
    match error {
        ScdcError::Transport(error) => Some(error.kind()),
        _ => None,
    }
}

/// An async TMDS bring-up at the SCDC layer: culvert-async writes the sink, the bench's PHY
/// starts scrambled output, and culvert-async reads the scrambler status after a delay.
async fn bring_up(
    scdc: &mut Client<'_>,
    phy: &mut Phy<'_>,
    delay: &mut Time<'_>,
) -> Result<bool, ScdcError<BusError>> {
    let config = TmdsConfig {
        scrambling_enable: true,
        high_tmds_clock_ratio: true,
    };
    scdc.write_tmds_config(config).await?;
    SendPhyPower::power_on(phy)
        .await
        .expect("the PHY powers on");
    SendTmdsPhy::configure(phy, high())
        .await
        .expect("it configures");
    SendTmdsPhy::set_scrambling(phy, true)
        .await
        .expect("it scrambles");
    SendTmdsPhy::start_output(phy).await.expect("it starts");
    SendDelay::delay(delay, Duration::from_millis(1)).await;
    Ok(scdc.read_scrambler_status().await?.scrambling_active)
}

#[test]
fn an_async_bring_up_locks_the_sink_on_another_thread() {
    let bench = Bench::new(SinkConfig::new(), &[]).unwrap();
    let mut scdc = client(&bench);
    let mut phy = bench.phy();
    let mut delay = bench.time();
    let future = bring_up(&mut scdc, &mut phy, &mut delay);
    // Spawning it proves the whole stack's future is `Send`.
    let locked = std::thread::scope(|s| s.spawn(move || pollster::block_on(future)).join());
    assert_eq!(locked.unwrap(), Ok(true));
    assert_eq!(bench.sink().tmds_config(), Some(0b11));
    assert_eq!(bench.sink().scrambling_status(), Some(1));
}

#[test]
fn update_flags_are_read_and_cleared_one_at_a_time() {
    let bench = Bench::new(SinkConfig::new(), &[]).unwrap();
    bench.preset_scdc(0x10, (1 << 5) | (1 << 1) | 1); // FLT_update, CED_update, Status_update
    let mut scdc = client(&bench);
    pollster::block_on(async {
        let flags = scdc.read_update_flags().await.unwrap();
        assert!(flags.flt_update && flags.ced_update && flags.status_update);
        let flt_only = ClearableUpdateFlags::new(false, false, false, false, true, false);
        scdc.clear_update_flags(flt_only).await.unwrap();
        let flags = scdc.read_update_flags().await.unwrap();
        assert!(!flags.flt_update && flags.ced_update && flags.status_update);
    });
    assert_eq!(bench.sink().update_0(), Some(0b11));
}

#[test]
fn versions_are_read_and_written() {
    let bench = Bench::new(SinkConfig::new().with_sink_version(1), &[]).unwrap();
    let mut scdc = client(&bench);
    pollster::block_on(async {
        assert_eq!(scdc.read_sink_version().await, Ok(1));
        scdc.write_source_version(1).await.unwrap();
    });
    assert_eq!(bench.sink().source_version(), Some(1));
}

#[test]
fn bus_errors_reach_the_client_with_their_kind() {
    let bench = Bench::new(SinkConfig::new(), &[]).unwrap();
    let mut scdc = client(&bench);
    for kind_in in [
        DdcErrorKind::NoAcknowledge(NoAcknowledgeSource::Data),
        DdcErrorKind::Busy,
        DdcErrorKind::Timeout,
        DdcErrorKind::Detached,
    ] {
        bench
            .schedule(Fault::new(Effect::Fail(kind_in)).on(Filter::ANY.with_addr(0x54)))
            .unwrap();
        let error = pollster::block_on(scdc.read_scrambler_status()).unwrap_err();
        assert_eq!(kind(&error), Some(kind_in));
        // The next read works.
        assert!(pollster::block_on(scdc.read_scrambler_status()).is_ok());
    }
    // A sink without SCDC does not answer.
    let bench = Bench::new(SinkConfig::new().without_scdc(), &[]).unwrap();
    let mut scdc = client(&bench);
    let error = pollster::block_on(scdc.read_sink_version()).unwrap_err();
    assert_eq!(
        kind(&error),
        Some(DdcErrorKind::NoAcknowledge(NoAcknowledgeSource::Address))
    );
}
