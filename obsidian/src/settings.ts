import { App, PluginSettingTab, Setting } from "obsidian";
import type { BookCompletion } from "./core/completions.ts";
import { TRANSLATIONS, type Translation } from "./core/literalWord.ts";
import type { StyleName } from "./core/settings.ts";
import type ToposPlugin from "./main.ts";

export { bookStyle, DEFAULT_SETTINGS, type StyleName, type ToposSettings } from "./core/settings.ts";

export class ToposSettingTab extends PluginSettingTab {
  private readonly plugin: ToposPlugin;

  constructor(app: App, plugin: ToposPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    const settings = this.plugin.settings;
    const save = () => this.plugin.saveSettings();
    containerEl.empty();

    new Setting(containerEl)
      .setName("Reference style")
      .setDesc("How completions, normalized references, and search results are written.")
      .addDropdown((d) =>
        d
          .addOptions({
            name: "Full name (John 3:16)",
            abbreviation: "Abbreviation (Jn 3:16)",
            osis: "OSIS (John.3.16)",
          })
          .setValue(settings.style)
          .onChange((value) => {
            settings.style = value as StyleName;
            void save();
          }),
      );

    new Setting(containerEl).setName("Literal Word").setHeading();
    new Setting(containerEl)
      .setName("Translation")
      .setDesc("References open at app.literalword.com in this translation.")
      .addDropdown((d) => {
        for (const t of TRANSLATIONS) d.addOption(t, t ? t.toUpperCase() : "Literal Word's default");
        d.setValue(settings.translation).onChange((value) => {
          settings.translation = value as Translation;
          void save();
        });
      });
    new Setting(containerEl)
      .setName("Link references in the editor")
      .setDesc("Underline references while editing, and open them in Literal Word when clicked.")
      .addToggle((t) =>
        t.setValue(settings.linkInEditor).onChange((value) => {
          settings.linkInEditor = value;
          void save();
        }),
      );
    new Setting(containerEl)
      .setName("Require Ctrl/Cmd to open from the editor")
      .setDesc("Plain clicks place the cursor; Ctrl/Cmd-click opens the reference.")
      .addToggle((t) =>
        t.setValue(settings.clickNeedsModifier).onChange((value) => {
          settings.clickNeedsModifier = value;
          void save();
        }),
      );
    new Setting(containerEl)
      .setName("Link references in reading view")
      .addToggle((t) =>
        t.setValue(settings.linkInReading).onChange((value) => {
          settings.linkInReading = value;
          void save();
        }),
      );

    new Setting(containerEl).setName("Autocomplete").setHeading();
    new Setting(containerEl)
      .setName("Suggest references while typing")
      .setDesc("After a book name, suggest chapters, then verses, then range ends.")
      .addToggle((t) =>
        t.setValue(settings.autocomplete).onChange((value) => {
          settings.autocomplete = value;
          void save();
        }),
      );
    new Setting(containerEl)
      .setName("Complete book names")
      .setDesc("In prose, completing every word would be noisy, so by default only words that start with a capital letter or number complete to book names.")
      .addDropdown((d) =>
        d
          .addOptions({ off: "Never", capitalized: "When capitalized", always: "Always" })
          .setValue(settings.bookCompletion)
          .onChange((value) => {
            settings.bookCompletion = value as BookCompletion;
            void save();
          }),
      );
    new Setting(containerEl)
      .setName("Number of suggestions")
      .addSlider((s) =>
        s
          .setLimits(5, 100, 5)
          .setValue(settings.suggestionLimit)
          .setDynamicTooltip()
          .onChange((value) => {
            settings.suggestionLimit = value;
            void save();
          }),
      );

    new Setting(containerEl).setName("Search").setHeading();
    new Setting(containerEl)
      .setName("File extensions")
      .setDesc("Files to search, separated by commas.")
      .addText((t) =>
        t.setValue(settings.extensions).onChange((value) => {
          settings.extensions = value;
          void save().then(() => this.plugin.reindex());
        }),
      );
    new Setting(containerEl)
      .setName("Excluded folders")
      .setDesc("One folder per line.")
      .addTextArea((t) =>
        t.setValue(settings.excludeFolders).onChange((value) => {
          settings.excludeFolders = value;
          void save().then(() => this.plugin.reindex());
        }),
      );
  }
}
