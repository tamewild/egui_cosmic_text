# egui_cosmic_text

A work in progress library that adds text functionality to [egui](https://github.com/emilk/egui) based on [cosmic text](https://github.com/pop-os/cosmic-text.git), including an optional text widget, a texture atlas, and drawing utilities.

### Widget

The widget, while optional, is included with the library by default.

There are rough edges to it right now, which will hopefully get better over time.

If you don't want to use the widget feature, you can use the library to draw text using cosmic text with the provided texture atlas (based on [glyphon](https://github.com/grovesNL/glyphon)). Drawing colored emojis, RTL, etc., is supported.

#### Widget Features
- Faster editing of large text compared to the default egui widget (thanks to `cosmic-text`)
- Blinking cursor
- Configurable single widget text selection
- Configurable context menu (copy, paste, cut, etc)

#### Widget Limitations
- No accessibility support yet
- No mobile support
- No IME support

### Additional Notes
This may not be the most optimal and performant implementation.

The implementation is based on [iced](https://github.com/iced-rs/iced), [glyphon](https://github.com/grovesNL/glyphon), [bevy_cosmic_edit](https://github.com/StaffEngineer/bevy_cosmic_edit), and [cosmic-edit](https://github.com/pop-os/cosmic-edit).

The emoji graphics come from [Twemoji](https://github.com/twitter/twemoji), licensed under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

The font file comes from [Emoji-COLRv0](https://github.com/Emoji-COLRv0/Emoji-COLRv0/).

Contributions would be appreciated!