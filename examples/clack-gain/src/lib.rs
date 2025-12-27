//! based on clack example gain gui plugin (https://github.com/prokopyl/clack/tree/main/plugin/examples/gain-gui#741e4d223e2d528b150834c8aca296702bd40dfb)

use crate::{gui::GainPluginGui, params::GainParams};
use clack_extensions::{
    audio_ports::*,
    gui::{GuiApiType, GuiConfiguration, PluginGui, PluginGuiImpl},
    params::*,
    state::PluginState,
};
use clack_plugin::prelude::*;
use std::sync::Arc;

mod gui;
mod params;

pub struct GainPlugin;

impl Plugin for GainPlugin {
    type AudioProcessor<'a> = GainPluginAudioProcessor<'a>;
    type Shared<'a> = GainPluginShared;
    type MainThread<'a> = GainPluginMainThread<'a>;

    fn declare_extensions(
        builder: &mut PluginExtensions<Self>,
        _shared: Option<&GainPluginShared>,
    ) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginParams>()
            .register::<PluginState>()
            .register::<PluginGui>();
    }
}

impl DefaultPluginFactory for GainPlugin {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::*;

        PluginDescriptor::new("org.rust-audio.clack.gain", "Clack Gain Example")
            .with_features([AUDIO_EFFECT, STEREO])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Self::Shared<'_>, PluginError> {
        Ok(GainPluginShared {
            params: Arc::new(GainParams::new()),
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        Ok(Self::MainThread {
            shared,
            gui: GainPluginGui::default(),
        })
    }
}

pub struct GainPluginAudioProcessor<'a> {
    shared: &'a GainPluginShared,
}

impl<'a> PluginAudioProcessor<'a, GainPluginShared, GainPluginMainThread<'a>>
    for GainPluginAudioProcessor<'a>
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &mut GainPluginMainThread,
        shared: &'a GainPluginShared,
        _audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self { shared })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let mut port_pair = audio
            .port_pair(0)
            .ok_or(PluginError::Message("No input/output ports found"))?;

        let mut output_channels = port_pair
            .channels()?
            .into_f32()
            .ok_or(PluginError::Message("Expected f32 input/output"))?;

        let mut channel_buffers = [None, None];

        for (pair, buf) in output_channels.iter_mut().zip(&mut channel_buffers) {
            *buf = match pair {
                ChannelPair::InputOnly(_) => None,
                ChannelPair::OutputOnly(_) => None,
                ChannelPair::InPlace(b) => Some(b),
                ChannelPair::InputOutput(i, o) => {
                    o.copy_from_slice(i);
                    Some(o)
                }
            }
        }

        for event_batch in events.input.batch() {
            for event in event_batch.events() {
                self.shared.params.handle_event(event)
            }

            let volume = self.shared.params.get_volume();
            for buf in channel_buffers.iter_mut().flatten() {
                for sample in buf[event_batch.sample_bounds()].iter_mut() {
                    *sample *= volume
                }
            }
        }

        Ok(ProcessStatus::ContinueIfNotQuiet)
    }
}

impl PluginAudioPortsImpl for GainPluginMainThread<'_> {
    fn count(&mut self, _is_input: bool) -> u32 {
        1
    }

    fn get(&mut self, index: u32, _is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"main",
                channel_count: 2,
                flags: AudioPortFlags::IS_MAIN,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: None,
            });
        }
    }
}

pub struct GainPluginShared {
    params: Arc<GainParams>,
}

impl PluginShared<'_> for GainPluginShared {}

pub struct GainPluginMainThread<'a> {
    shared: &'a GainPluginShared,
    gui: GainPluginGui,
}

impl<'a> PluginMainThread<'a, GainPluginShared> for GainPluginMainThread<'a> {}

clack_export_entry!(SinglePluginEntry<GainPlugin>);

impl<'a> PluginGuiImpl for GainPluginMainThread<'a> {
    fn is_api_supported(&mut self, configuration: clack_extensions::gui::GuiConfiguration) -> bool {
        configuration.api_type
            == GuiApiType::default_for_current_platform().expect("Unsupported platform")
            && !configuration.is_floating
    }

    fn get_preferred_api(&'_ mut self) -> Option<clack_extensions::gui::GuiConfiguration<'_>> {
        Some(GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform().expect("Unsupported platform"),
            is_floating: false,
        })
    }

    fn create(
        &mut self,
        _configuration: clack_extensions::gui::GuiConfiguration,
    ) -> Result<(), PluginError> {
        Ok(())
    }

    fn destroy(&mut self) {
        self.gui.close();
    }

    fn set_scale(&mut self, _scale: f64) -> Result<(), PluginError> {
        Ok(())
    }

    fn get_size(&mut self) -> Option<clack_extensions::gui::GuiSize> {
        Some(clack_extensions::gui::GuiSize {
            width: 400,
            height: 200,
        })
    }

    fn set_size(&mut self, _size: clack_extensions::gui::GuiSize) -> Result<(), PluginError> {
        Ok(())
    }

    fn set_parent(&mut self, window: clack_extensions::gui::Window) -> Result<(), PluginError> {
        self.gui.open(self.shared, window)?;
        Ok(())
    }

    fn set_transient(&mut self, _window: clack_extensions::gui::Window) -> Result<(), PluginError> {
        Ok(())
    }

    fn show(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    fn hide(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
