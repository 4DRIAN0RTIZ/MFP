# Changelog

All notable changes to this project are documented here.

## [Unreleased]

### Documentation

- Point landing page links to mfp.cuevaneander.tech ([a8ecc90](https://github.com/4DRIAN0RTIZ/MFP/commit/a8ecc904a4488c741f231e1777bf738cce70f938))

### Features

- Add English and Spanish i18n to the landing page ([9b8f748](https://github.com/4DRIAN0RTIZ/MFP/commit/9b8f7480aa4b8856324bc16a4786c72f6e867cca))
- Redesign the landing page with an interactive demo, themes, Open Graph and release notes ([87ca9d4](https://github.com/4DRIAN0RTIZ/MFP/commit/87ca9d49ab453a1405bf3bd2be03f7ae93b090f4))

## [0.3.0] - 2026-09-26

### Build

- Bump crossterm to 0.28 and add ratatui ([01c7358](https://github.com/4DRIAN0RTIZ/MFP/commit/01c73589bfc41c8c9bb112888124476a0245895e))

### CI

- Add git-cliff changelog and automated release workflows ([e88bf89](https://github.com/4DRIAN0RTIZ/MFP/commit/e88bf893f3004dfec5674bfe2422f56a879f258c))

### Chore

- Update CHANGELOG.md and docs/changelog.json [skip ci] ([8c365cc](https://github.com/4DRIAN0RTIZ/MFP/commit/8c365cca9879f577b4ffcfb1db72c44a8c4f7f9c))
- Update ([f0246b0](https://github.com/4DRIAN0RTIZ/MFP/commit/f0246b01929c7a71f8b72605939b5e64fb0b68c5))

### Documentation

- Document the visualizer, themes and configuration on the landing page ([5847b58](https://github.com/4DRIAN0RTIZ/MFP/commit/5847b58a1659c425ea1d758aaab07d5acdef7240))

### Features

- Rebind h/v/V keys and read visualizer settings from config.toml ([427e26d](https://github.com/4DRIAN0RTIZ/MFP/commit/427e26d156fd6b6087f210c3892952064abb5286))
- Add config.toml and semantic-role themes with built-in presets ([942e051](https://github.com/4DRIAN0RTIZ/MFP/commit/942e05141134d5cbe0e27b0203a23d27a68cc015))
- Show a live audio visualizer with switchable styles ([9750897](https://github.com/4DRIAN0RTIZ/MFP/commit/9750897dc5a84471cf411b4d269be72071d42a95))
- Add visualizer styles (bars, mirror, wave, dots, area, vu) ([c58a060](https://github.com/4DRIAN0RTIZ/MFP/commit/c58a060b59fd79d9d6f9392ed502b9d8f8f0cc1e))
- Add audio tap and hand-written FFT spectrum analyzer ([739ec83](https://github.com/4DRIAN0RTIZ/MFP/commit/739ec838788e11b2ad20a531e3d68cdf5b6a4057))
- Make the TUI the default and keep the text UI behind --plain ([96dac4c](https://github.com/4DRIAN0RTIZ/MFP/commit/96dac4ca88cbdf2374bdbe4ab33a5d6ddff12bb3))
- Add episode list, search and compact/full layouts ([5f9aa59](https://github.com/4DRIAN0RTIZ/MFP/commit/5f9aa5931a12f937b4a2491c9413ed18bc6e05e4))
- Add ratatui base with compact player view behind --tui ([8f20119](https://github.com/4DRIAN0RTIZ/MFP/commit/8f2011902c3b70d1807ff9f7522a03946a140e9e))
- Show latest release version on the landing page ([3542373](https://github.com/4DRIAN0RTIZ/MFP/commit/35423736d25172a703ca20719d3fc10418c75164))
- Added landing page ([c5fae0f](https://github.com/4DRIAN0RTIZ/MFP/commit/c5fae0fc1c1cfd03ea9ec641c66f4839e54ccdb7))

### Refactor

- Unify keyboard and mpris actions and make mpris optional ([3c9d501](https://github.com/4DRIAN0RTIZ/MFP/commit/3c9d50118deebe49536b8a56be86879f807b9e3e))
- Remove terminal output from operations, player and mpris ([8f7c4cf](https://github.com/4DRIAN0RTIZ/MFP/commit/8f7c4cfa1f191cc41bda5679e9c8814a5694d424))
- Introduce models, config and operations layers ([7b534ee](https://github.com/4DRIAN0RTIZ/MFP/commit/7b534ee1d2b82dc67bef76045bae9cdce01d155b))
- Extract clap and command handlers into cli module ([5ef786a](https://github.com/4DRIAN0RTIZ/MFP/commit/5ef786af3de84e2aba3c0ad24b71470d8a46ac37))

### Testing

- Add characterization tests for playlist, favorites and parsing ([7974636](https://github.com/4DRIAN0RTIZ/MFP/commit/7974636963d562028b9e765caf264525d0e75c63))

## [0.2.0] - 2026-04-08

### Features

- Add MPRIS D-Bus integration for Linux media controls ([7559900](https://github.com/4DRIAN0RTIZ/MFP/commit/7559900df4208e61526122fc73e0b1c0e77eb57e))

## [0.1.0] - 2026-03-04

### CI

- Use native arm64 runners, fix macos x86_64 cross-compile ([5f3649f](https://github.com/4DRIAN0RTIZ/MFP/commit/5f3649f591f66f631c58d3a323f47ff7a471a8b7))
- Install libasound2-dev for Linux native builds ([5670b87](https://github.com/4DRIAN0RTIZ/MFP/commit/5670b87affa25291dda3cd5fedb65bff56f0f3bf))
- Add release workflow for Linux and macOS binaries ([3f287e8](https://github.com/4DRIAN0RTIZ/MFP/commit/3f287e81380160b829b44ff4a22381e2c6d3e419))


