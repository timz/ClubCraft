//! Shared browser preview identity and presentation.
use clubcraft_core::TrackId;
use clubcraft_engine::command::PreviewOutput;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewSource {
    Library {
        playlist: Option<i64>,
        mix: Option<i64>,
    },
    LiveBrowser {
        playlist: Option<i64>,
        mix: Option<i64>,
    },
    LiveQueue,
    Playlist,
    Mashups,
    AddTracks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewItem {
    pub source: PreviewSource,
    pub track: TrackId,
    pub row: usize,
}

#[derive(Default)]
pub struct PreviewState {
    pub item: Option<PreviewItem>,
    pub output: PreviewOutput,
    pub loading: bool,
    pub mix_serial: u64,
    pub mashup_revision: u64,
}

impl PreviewState {
    pub fn output_label(&self) -> &'static str { self.output.label() }
    pub fn playing(&self, source: PreviewSource, track: TrackId, row: usize) -> bool {
        self.item == Some(PreviewItem { source, track, row })
    }

    pub fn label(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(format!("Preview: {}", self.output_label()))
                .small()
                .color(crate::theme::TEXT_DIM),
        );
    }

    pub fn button(&self, ui: &mut egui::Ui, item: PreviewItem, enabled: bool) -> egui::Response {
        let playing = self.playing(item.source, item.track, item.row);
        crate::widgets::preview_button(ui, playing, enabled, crate::theme::PREVIEW_BUTTON_SIZE)
            .on_hover_text(if playing {
                if self.loading {
                    "Loading preview - click to stop".to_string()
                } else {
                    "Stop preview".to_string()
                }
            } else if item.source == PreviewSource::Mashups {
                format!(
                    "Preview vocals over this instrumental in {}",
                    self.output_label()
                )
            } else {
                format!("Preview in {} from the first drop", self.output_label())
            })
    }
}

/// Auto uses the running endpoint; a configured but failed headphone route is never a speaker fallback.
pub fn target_output(preference: PreviewOutput, config: &clubcraft_engine::AudioConfig, info: &clubcraft_engine::StreamInfo) -> PreviewOutput {
    use clubcraft_engine::HeadphoneRouting as H;
    if preference != PreviewOutput::Auto { return preference; }
    if info.cue_channels.is_some() || matches!(config.headphones, H::SeparateDevice | H::Channels { .. } | H::Split) {
        PreviewOutput::Headphones
    } else {
        PreviewOutput::Master
    }
}

pub fn output_available(output: PreviewOutput, info: &clubcraft_engine::StreamInfo) -> bool {
    match output {
        PreviewOutput::Auto => false,
        PreviewOutput::Master => !info.master.failed,
        PreviewOutput::Headphones => headphones_available(info),
    }
}

pub fn resolve_output(preference: PreviewOutput, config: &clubcraft_engine::AudioConfig, info: &clubcraft_engine::StreamInfo) -> anyhow::Result<PreviewOutput> {
    let output = target_output(preference, config, info);
    if info.master.failed { anyhow::bail!("Audio output is unavailable. Apply audio settings to retry."); }
    if output == PreviewOutput::Headphones && !headphones_available(info) {
        anyhow::bail!("Headphone output is unavailable. Configure Audio output in Settings.");
    }
    Ok(output)
}

pub fn headphones_available(info: &clubcraft_engine::StreamInfo) -> bool {
    info.cue_channels.is_some()
        && !info.master.failed
        && info.headphones.as_ref().is_none_or(|h| !h.failed)
}

#[cfg(test)]
pub(crate) mod test_ui {
    use eframe::egui::{self, Event, Modifiers, PointerButton, RawInput};

    pub fn click_preview(mut draw: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let mut frame = |events: Vec<Event>| {
            let input = RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 900.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| draw(ui));
            output.textures_delta.clear();
        };
        frame(vec![]);
        frame(vec![]);
        let rect = ctx
            .interactive_rects_last_pass()
            .into_iter()
            .find(|r| {
                (r.width() - crate::theme::PREVIEW_BUTTON_SIZE).abs() < 0.1
                    && (r.height() - crate::theme::PREVIEW_BUTTON_SIZE).abs() < 0.1
            })
            .expect("preview button is visible");
        let pos = rect.center();
        frame(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ]);
        frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clubcraft_engine::{EndpointInfo, StreamInfo};

    fn output() -> StreamInfo {
        let endpoint = EndpointInfo {
            device: "test".into(),
            id: "test".into(),
            sample_rate: 44100,
            channels: 2,
            sample_format: "f32".into(),
            requested_buffer_frames: None,
            observed_buffer_frames: None,
            estimated_latency_ms: None,
            failed: false,
            errors: 0,
        };
        StreamInfo {
            device: "test".into(),
            sample_rate: 44100,
            channels: 2,
            buffer_frames: None,
            master_channels: [0, 1],
            cue_channels: None,
            split: false,
            master: endpoint,
            headphones: None,
            warning: None,
            cue_underruns: 0,
            cue_overflows: 0,
            estimated_offset_ms: None,
            master_delay_ms: 0.0,
            cue_delay_ms: 0.0,
        }
    }

    #[test]
    fn automatic_output_uses_running_routes_and_never_falls_back_from_a_failed_route() {
        use clubcraft_engine::{AudioConfig, HeadphoneRouting as H};
        let mut info = output();
        let mut cfg = AudioConfig::default();
        assert_eq!(cfg.headphones, H::Auto);
        assert_eq!(resolve_output(PreviewOutput::Auto, &cfg, &info).unwrap(), PreviewOutput::Master);
        assert!(resolve_output(PreviewOutput::Headphones, &cfg, &info).is_err());
        info.cue_channels = Some([2, 3]);
        assert_eq!(resolve_output(PreviewOutput::Auto, &cfg, &info).unwrap(), PreviewOutput::Headphones);
        info.headphones = Some(info.master.clone());
        info.headphones.as_mut().unwrap().failed = true;
        assert!(resolve_output(PreviewOutput::Auto, &cfg, &info).is_err());
        info = output();
        cfg.headphones = H::SeparateDevice;
        assert!(resolve_output(PreviewOutput::Auto, &cfg, &info).is_err());
        // Legacy shared output has no headphone endpoint and cannot carry deck CUE.
        cfg.headphones = H::MainOutput;
        assert_eq!(resolve_output(PreviewOutput::Auto, &cfg, &info).unwrap(), PreviewOutput::Master);
        info.master.failed = true;
        assert!(resolve_output(PreviewOutput::Auto, &cfg, &info).is_err());
    }

    #[test]
    fn loss_does_not_retarget_an_active_or_loading_preview() {
        use clubcraft_engine::AudioConfig;
        let cfg = AudioConfig::default();
        let mut info = output();
        info.cue_channels = Some([2, 3]);
        let active = resolve_output(PreviewOutput::Auto, &cfg, &info).unwrap();
        assert_eq!(active, PreviewOutput::Headphones);
        info.cue_channels = None;
        assert!(!output_available(active, &info));
        // Only a new request on a running stereo fallback can resolve to speakers.
        assert_eq!(resolve_output(PreviewOutput::Auto, &cfg, &info).unwrap(), PreviewOutput::Master);
    }

    #[test]
    fn preview_requires_a_running_cue_output_not_just_a_saved_selection() {
        let mut info = output();
        assert!(!headphones_available(&info));
        info.cue_channels = Some([2, 3]);
        assert!(headphones_available(&info));
        info.split = true;
        info.cue_channels = Some([0, 1]);
        assert!(headphones_available(&info));
        info.headphones = Some(info.master.clone());
        assert!(headphones_available(&info));
        info.headphones.as_mut().unwrap().failed = true;
        assert!(!headphones_available(&info));
        info.headphones = None;
        info.master.failed = true;
        assert!(!headphones_available(&info));
    }
}
