use hdmi_hal_async::scdc::ScdcTransport;

use crate::{ReadModifyWriteError, ScdcError};
use crate::{ScramblerStatus, TmdsConfig, TmdsConfigReading};
use culvert::codec;

use super::Scdc;

impl<T: ScdcTransport> Scdc<T> {
    /// Writes scrambling configuration to `TMDS_Config` (0x20).
    ///
    /// Sets `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1).
    pub async fn write_tmds_config(
        &mut self,
        config: TmdsConfig,
    ) -> Result<(), ScdcError<T::Error>> {
        self.transport
            .write(codec::TMDS_CONFIG, codec::encode_tmds_config(config))
            .await
            .map_err(ScdcError::Transport)
    }

    /// Reads `TMDS_Config` (0x20): the two bits [`write_tmds_config`](Self::write_tmds_config)
    /// sets, and the whole byte.
    pub async fn read_tmds_config(&mut self) -> Result<TmdsConfigReading, ScdcError<T::Error>> {
        let byte = self
            .transport
            .read(codec::TMDS_CONFIG)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_tmds_config_reading(byte))
    }

    /// Sets `Scrambling_Enable` (bit 0) and `TMDS_Bit_Clock_Ratio` (bit 1) of `TMDS_Config`
    /// (0x20) and keeps its other bits: reads the byte, then writes it back with the two
    /// bits from `config` and every other bit as read.
    ///
    /// [`write_tmds_config`](Self::write_tmds_config) writes the whole byte instead, the
    /// other bits 0. Two transfers, not one transaction: a failed read writes nothing
    /// ([`ReadModifyWriteError::Read`]), and a failed write says so
    /// ([`ReadModifyWriteError::Write`]).
    pub async fn update_tmds_config(
        &mut self,
        config: TmdsConfig,
    ) -> Result<(), ReadModifyWriteError<T::Error>> {
        let byte = self
            .transport
            .read(codec::TMDS_CONFIG)
            .await
            .map_err(|e| ReadModifyWriteError::Read(ScdcError::Transport(e)))?;
        self.transport
            .write(
                codec::TMDS_CONFIG,
                codec::update_tmds_config_byte(byte, config),
            )
            .await
            .map_err(|e| ReadModifyWriteError::Write(ScdcError::Transport(e)))
    }

    /// Reads scrambler acknowledgement from `Scrambler_Status` (0x21).
    ///
    /// Returns [`ScramblerStatus::scrambling_active`] set when the sink confirms
    /// that TMDS scrambling is active (bit 0).
    pub async fn read_scrambler_status(&mut self) -> Result<ScramblerStatus, ScdcError<T::Error>> {
        let byte = self
            .transport
            .read(codec::SCRAMBLER_STATUS)
            .await
            .map_err(ScdcError::Transport)?;
        Ok(codec::decode_scrambler_status(byte))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scdc;
    use super::super::test_transport::{Op, TestTransport};
    use crate::TmdsConfig;
    use crate::{ReadModifyWriteError, ScdcError};

    const SCRAMBLED_40: TmdsConfig = TmdsConfig {
        scrambling_enable: true,
        high_tmds_clock_ratio: true,
    };

    #[pollster::test]
    async fn read_tmds_config_decodes_the_two_bits_and_keeps_the_byte() {
        let mut sim = TestTransport::new();
        sim.set(0x20, 0xA6); // ratio set, scrambling clear, reserved bits set
        let reading = Scdc::new(sim).read_tmds_config().await.unwrap();
        let expected = TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: true,
        };
        assert_eq!(reading.config, expected);
        assert_eq!(reading.raw, 0xA6);
    }

    #[pollster::test]
    async fn update_tmds_config_keeps_the_other_bits() {
        let mut sim = TestTransport::new();
        sim.set(0x20, 0xF0);
        let mut scdc = Scdc::new(sim);
        scdc.update_tmds_config(SCRAMBLED_40).await.unwrap();
        let sim = scdc.into_transport();
        assert_eq!(sim.get(0x20), 0xF3);
        assert_eq!(
            sim.log()[..3],
            [Some(Op::Read(0x20)), Some(Op::Write(0x20, 0xF3)), None]
        );
    }

    #[pollster::test]
    async fn update_tmds_config_clears_only_the_two_bits() {
        let mut sim = TestTransport::new();
        sim.set(0x20, 0xFF);
        let mut scdc = Scdc::new(sim);
        scdc.update_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: false,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0xFC);
    }

    #[pollster::test]
    async fn update_tmds_config_writes_nothing_after_a_failed_read() {
        let mut scdc = Scdc::new(TestTransport::failing_after(0));
        assert!(
            scdc.update_tmds_config(SCRAMBLED_40)
                .await
                .is_err_and(|error| matches!(
                    error,
                    ReadModifyWriteError::Read(ScdcError::Transport(()))
                ))
        );
        assert_eq!(scdc.into_transport().log()[0], None);
    }

    #[pollster::test]
    async fn update_tmds_config_reports_a_failed_write() {
        let mut sim = TestTransport::failing_after(1);
        sim.set(0x20, 0x80);
        let mut scdc = Scdc::new(sim);
        assert!(
            scdc.update_tmds_config(SCRAMBLED_40)
                .await
                .is_err_and(|error| matches!(
                    error,
                    ReadModifyWriteError::Write(ScdcError::Transport(()))
                ))
        );
        let sim = scdc.into_transport();
        assert_eq!(sim.log()[..2], [Some(Op::Read(0x20)), None]);
        assert_eq!(sim.get(0x20), 0x80);
    }

    #[pollster::test]
    async fn tmds_config_scrambling_only() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: true,
            high_tmds_clock_ratio: false,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x01);
    }

    #[pollster::test]
    async fn tmds_config_clock_ratio_only() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: true,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x02);
    }

    #[pollster::test]
    async fn tmds_config_both_clear() {
        let mut scdc = Scdc::new(TestTransport::new());
        scdc.write_tmds_config(TmdsConfig {
            scrambling_enable: false,
            high_tmds_clock_ratio: false,
        })
        .await
        .unwrap();
        assert_eq!(scdc.into_transport().get(0x20), 0x00);
    }

    #[pollster::test]
    async fn scrambler_status_bit0_set() {
        let mut sim = TestTransport::new();
        sim.set(0x21, 0xFF); // all bits set; only bit 0 is defined
        let status = Scdc::new(sim).read_scrambler_status().await.unwrap();
        // The bit decoded, and the byte as read, reserved bits included.
        assert!(status.scrambling_active);
        assert_eq!(status.raw, 0xFF);
    }

    #[pollster::test]
    async fn scrambler_status_bit0_clear() {
        let mut sim = TestTransport::new();
        sim.set(0x21, 0xFE); // bit 0 clear, other bits set (reserved)
        let status = Scdc::new(sim).read_scrambler_status().await.unwrap();
        // The bit decoded, and the byte as read, reserved bits included.
        assert!(!status.scrambling_active);
        assert_eq!(status.raw, 0xFE);
    }

    #[pollster::test]
    async fn transport_error_propagates() {
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .write_tmds_config(TmdsConfig {
                    scrambling_enable: false,
                    high_tmds_clock_ratio: false
                })
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_scrambler_status()
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
        assert!(
            Scdc::new(TestTransport::failing_after(0))
                .read_tmds_config()
                .await
                .is_err_and(|error| matches!(error, ScdcError::Transport(())))
        );
    }
}
