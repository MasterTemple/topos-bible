import { App, Notice, type DropdownComponent, Platform, PluginSettingTab, Setting } from "obsidian";
import { BookStyle } from "topos-bible";
import { DEFAULT_FORMAT, written, type FormatSettings } from "./core/format.ts";
import { parseQuery } from "./core/query.ts";
import type { BookCompletion } from "./core/completions.ts";
import { FILTERS, linkUrl, PLACEHOLDERS, SITES, templateError } from "./core/links.ts";
import { bookStyle, type StyleName } from "./core/settings.ts";
import type ToposPlugin from "./main.ts";

export { bookStyle, DEFAULT_SETTINGS, type StyleName, type ToposSettings } from "./core/settings.ts";

export class ToposSettingTab extends PluginSettingTab {
  private readonly plugin: ToposPlugin;

  constructor(app: App, plugin: ToposPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  /** Settings open in their own window, so redraw the notes behind it once more on closing */
  hide(): void {
    this.plugin.refreshLinks();
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

    this.links(containerEl);
    new Setting(containerEl)
      .setName("Link references in the editor")
      .setDesc("Highlight references while editing, and open their links when clicked.")
      .addToggle((t) =>
        t.setValue(settings.linkInEditor).onChange((value) => {
          settings.linkInEditor = value;
          void save();
        }),
      );
    new Setting(containerEl)
      .setName("Require Ctrl/Cmd to open references")
      .setDesc(
        "On desktop, plain clicks place the cursor (or do nothing in reading view); Ctrl/Cmd-click opens the reference.",
      )
      .addToggle((t) =>
        t.setValue(settings.clickNeedsModifier).onChange((value) => {
          settings.clickNeedsModifier = value;
          void save();
        }),
      );
    new Setting(containerEl)
      .setName("Tap to open references in the editor")
      .setDesc(
        "On mobile, the tap that would place the cursor and open the keyboard opens the reference's link instead. Once you're typing, taps move the cursor as usual. Taps outside the editor always open references.",
      )
      .addToggle((t) =>
        t.setValue(settings.tapOpensInEditor).onChange((value) => {
          settings.tapOpensInEditor = value;
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
    this.referenceColor(containerEl);

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

    this.formatSettings(containerEl);

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
      .setName("Search EPUB files")
      .setDesc(
        `Find references in EPUBs too. Needs the EPUB++ plugin, which opens EPUBs in Obsidian: it shows their references in the book, with a sidebar tab to step through them and save one as a highlight. Results open in the book.${this.plugin.epubs.api ? "" : " (EPUB++ isn't enabled.)"}`,
      )
      .addToggle((t) =>
        t.setValue(settings.searchEpubs).onChange((value) => {
          settings.searchEpubs = value;
          void save().then(() => this.plugin.epubs.toggled());
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

  /**
   * How references are written (the CLI's --psg-fmt fields), with a preview: completions, the
   * sidebar, the dialogs, and "Normalize references" all use it
   */
  /** The references' color: the accent color until one is picked */
  private referenceColor(containerEl: HTMLElement): void {
    const settings = this.plugin.settings;
    new Setting(containerEl)
      .setName("Reference color")
      .setDesc(
        settings.referenceColor
          ? "The glow, dashed underline, and hover highlight of references."
          : "The glow, dashed underline, and hover highlight of references. Your accent color until you pick one.",
      )
      .addColorPicker((c) =>
        c.setValue(settings.referenceColor || accentColor(containerEl)).onChange((value) => {
          settings.referenceColor = value;
          void this.plugin.saveSettings();
        }),
      )
      .addExtraButton((b) =>
        b
          .setIcon("rotate-ccw")
          .setTooltip("Use the accent color")
          .onClick(() => {
            settings.referenceColor = "";
            void this.plugin.saveSettings();
            this.display();
          }),
      );
  }

  /** Where references open: a site, a template of your own, or nowhere */
  private links(containerEl: HTMLElement): void {
    const settings = this.plugin.settings;
    const choice = (template: string) =>
      !template.trim() ? "none" : (SITES.find((site) => site.template === template)?.id ?? "custom");
    new Setting(containerEl)
      .setName("Links")
      .setHeading()
      .setDesc(
        `A link template is an address with placeholders: ${Object.keys(PLACEHOLDERS)
          .map((name) => `{${name}}`)
          .join(" ")}. Filters change a value: {book|${Object.keys(FILTERS).join("|")}}. ` +
          "A part in [ ] is left out when a placeholder in it has no value, like {verse} for a whole chapter.",
      );
    let sites: DropdownComponent | undefined;
    new Setting(containerEl)
      .setName("Open references in")
      .setDesc("Where clicking a reference, the sidebar's ↗ button, and reading view links go.")
      .addDropdown((d) => {
        sites = d;
        for (const site of SITES) d.addOption(site.id, site.name);
        d.addOption("custom", "A link template");
        d.addOption("none", "Nowhere (no links)");
        d.setValue(choice(settings.linkTemplate)).onChange((value) => {
          const site = SITES.find((s) => s.id === value);
          if (site) settings.linkTemplate = site.template;
          if (value === "none") settings.linkTemplate = "";
          if (value === "custom" && !settings.linkTemplate.trim()) settings.linkTemplate = "https://";
          void this.plugin.saveSettings();
          this.display();
        });
      });
    if (!settings.linkTemplate.trim()) return;
    const template = new Setting(containerEl).setName("Link template");
    const preview = () => {
      const error = templateError(settings.linkTemplate);
      const examples = ["John 3:16-18", "Psalm 23"].map((reference) => {
        const passage = this.plugin.topos.parse(reference, BookStyle.Name);
        return (passage && linkUrl(settings.linkTemplate, passage)) ?? `no link for ${reference}`;
      });
      template.setDesc(error ? `Can't be used: ${error}` : `Preview: ${examples.join("   ")}`);
      template.descEl.toggleClass("topos-query-error", error !== null);
    };
    template.addText((t) => {
      t.inputEl.addClass("topos-query-input");
      t.setPlaceholder("https://example.com/{book.id}/{chapter}[/{verse}]")
        .setValue(settings.linkTemplate)
        .onChange((value) => {
          settings.linkTemplate = value;
          sites?.setValue(choice(value));
          preview();
          void this.plugin.saveSettings();
        });
    });
    preview();
  }

  private formatSettings(containerEl: HTMLElement): void {
    const { settings } = this.plugin;
    const format = settings.format;
    const heading = new Setting(containerEl).setName("Reference format").setHeading();
    const preview = () => {
      const topos = this.plugin.topos;
      const style = bookStyle(settings.style);
      const samples = ["John 3:16,17,18; 4:1-5:3", "Jude 5"]
        .map((text) => topos.parse(text, BookStyle.Name))
        .filter((passage) => passage !== null)
        .map((passage) => written(topos, passage, style, format));
      heading.setDesc(`Preview: ${samples.join("   ")}`);
    };
    const save = () => {
      void this.plugin.saveSettings();
      preview();
    };
    const texts: [keyof FormatSettings, string, string][] = [
      ["bookSeparator", "Between the book and chapter", "John␣3:16"],
      ["chapterVerse", "Between chapter and verse", "3:16"],
      ["range", "Range", "3:16-18"],
      ["verseSeparator", "Between verses in a chapter", "3:16,18"],
      ["chapterSeparator", "Between chapters", "3:16; 4:1"],
    ];
    for (const [key, name, example] of texts) {
      new Setting(containerEl)
        .setName(name)
        .setDesc(example)
        .addText((t) =>
          t
            .setPlaceholder(JSON.stringify(DEFAULT_FORMAT[key]))
            .setValue(String(format[key]))
            .onChange((value) => {
              // Separators can be spaces, so only an empty field means the default
              (format[key] as string) = value === "" ? (DEFAULT_FORMAT[key] as string) : value;
              save();
            }),
        );
    }
    const toggles: [keyof FormatSettings, string, string][] = [
      ["joinAdjacent", "Join adjacent verses", "3:16-18, not 3:16,17,18"],
      ["omitFirstVerseOfChapterRange", "Leave out the first verse of chapter ranges", "1-2:3, not 1:1-2:3"],
      ["chapterInSingleChapterBooks", "Chapter in single-chapter books", "Jude 1:5, not Jude 5"],
    ];
    for (const [key, name, example] of toggles) {
      new Setting(containerEl)
        .setName(name)
        .setDesc(example)
        .addToggle((t) =>
          t.setValue(format[key] as boolean).onChange((value) => {
            (format[key] as boolean) = value;
            save();
          }),
        );
    }
    new Setting(containerEl).addButton((b) =>
      b.setButtonText("Reset the format").onClick(() => {
        settings.format = { ...DEFAULT_FORMAT };
        void this.plugin.saveSettings();
        this.display();
      }),
    );
    preview();
  }

  /** Each saved search is a name and the CLI's filter options, editable here */
  private savedSearches(containerEl: HTMLElement): void {
    const { settings } = this.plugin;
    new Setting(containerEl)
      .setName("Saved searches")
      .setHeading()
      .setDesc(
          'Filters written like the topos CLI\'s options, plus an optional folder: Sermons --nt -g "Pauline Epistles" -b John --exclude-book Philemon -i "Romans 8" -o "John 1" --exclude-overlap "Psalm 23" (-o is explicit overlap; --any-overlap counts whole chapters too, and --exact-overlap means exactly that passage). Save the sidebar\'s filters with its Save button, or add one here.',
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

/** The theme's accent color as `#rrggbb` (what the color picker takes) */
function accentColor(el: HTMLElement): string {
  try {
    const probe = el.createDiv();
    probe.style.color = "var(--text-accent)";
    const color = getComputedStyle(probe).color;
    probe.remove();
    // `rgb(216, 180, 254)`, or `color(srgb 0.85 0.71 1)` with 0-1 channels
    const scale = color.startsWith("color(") ? 255 : 1;
    const channels = (color.match(/\d*\.?\d+/g) ?? []).slice(0, 3).map((n) => Number(n) * scale);
    if (channels.length < 3) return "#000000";
    return `#${channels.map((n) => Math.round(n).toString(16).padStart(2, "0")).join("")}`;
  } catch {
    return "#000000";
  }
}
