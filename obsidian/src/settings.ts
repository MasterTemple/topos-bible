import { App, Notice, Platform, PluginSettingTab, Setting } from "obsidian";
import { parseQuery } from "./core/query.ts";
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
    if (Platform.isDesktopApp) {
      new Setting(containerEl)
        .setName("Search engine")
        .setDesc(
          "Built in: indexes in a background thread, everywhere. topos CLI: runs the native command-line tool, which is faster for very large vaults (install it with cargo install topos-bible-cli). Edits are always indexed by the built-in engine.",
        )
        .addDropdown((d) =>
          d
            .addOptions({ builtin: "Built in", cli: "topos CLI" })
            .setValue(settings.engine)
            .onChange((value) => {
              settings.engine = value as "builtin" | "cli";
              void save().then(() => {
                this.display();
                void this.plugin.reindex();
              });
            }),
        );
      if (settings.engine === "cli") {
        new Setting(containerEl)
          .setName("topos command")
          .setDesc("Leave empty to use ~/.cargo/bin/topos, or topos from PATH.")
          .addText((t) =>
            t
              .setPlaceholder(this.plugin.cliPath())
              .setValue(settings.cliPath)
              .onChange((value) => {
                settings.cliPath = value.trim();
                void save();
              }),
          )
          .addButton((b) =>
            b.setButtonText("Test").onClick(() => {
              void this.plugin.testCli().then((message) => new Notice(message));
            }),
          );
        new Setting(containerEl)
          .setName("Cache CLI results")
          .setDesc("Reuse results for files that have not changed since the last run (topos --cache).")
          .addToggle((t) =>
            t.setValue(settings.cliCache).onChange((value) => {
              settings.cliCache = value;
              void save();
            }),
          );
      }
    }

    new Setting(containerEl)
      .setName("Excluded folders")
      .setDesc("One folder per line.")
      .addTextArea((t) =>
        t.setValue(settings.excludeFolders).onChange((value) => {
          settings.excludeFolders = value;
          void save().then(() => this.plugin.reindex());
        }),
      );

    this.savedSearches(containerEl);
  }

  /** Each saved search is a name and the CLI's filter options, editable here */
  private savedSearches(containerEl: HTMLElement): void {
    const { settings } = this.plugin;
    new Setting(containerEl)
      .setName("Saved searches")
      .setHeading()
      .setDesc(
          'Filters written like the topos CLI\'s options, plus an optional folder: Sermons --nt -g "Pauline Epistles" -b John --exclude-book Philemon -i "Romans 8" -o "John 1" --outside "Psalm 23". Save the sidebar\'s filters with its Save button, or add one here.',
      );
    const persist = () => {
      void this.plugin.saveData(settings).then(() => this.plugin.search.set({}));
    };
    settings.queries.forEach((saved, i) => {
      const setting = new Setting(containerEl);
      const showErrors = () => {
        const { errors } = parseQuery(saved.query);
        setting.setDesc(errors.join("; "));
        setting.descEl.toggleClass("topos-query-error", errors.length > 0);
      };
      setting
        .addText((t) =>
          t
            .setPlaceholder("Name")
            .setValue(saved.name)
            .onChange((value) => {
              saved.name = value;
              persist();
            }),
        )
        .addText((t) => {
          t.inputEl.addClass("topos-query-input");
          t.setPlaceholder('--nt -g "Pauline Epistles"')
            .setValue(saved.query)
            .onChange((value) => {
              saved.query = value;
              showErrors();
              persist();
            });
        })
        .addExtraButton((b) =>
          b
            .setIcon("search")
            .setTooltip("Show in the sidebar")
            .onClick(() => void this.plugin.openQuery(saved)),
        )
        .addExtraButton((b) =>
          b
            .setIcon("trash")
            .setTooltip("Delete")
            .onClick(() => {
              settings.queries.splice(i, 1);
              persist();
              this.display();
            }),
        );
      showErrors();
    });
    new Setting(containerEl).addButton((b) =>
      b.setButtonText("Add a saved search").onClick(() => {
        settings.queries.push({ name: `Search ${settings.queries.length + 1}`, query: "" });
        persist();
        this.display();
      }),
    );
  }
}
