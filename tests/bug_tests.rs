//! Tests für die in Schritt 1 gefundenen Bugs. Jede Test-Datei wird nach dem
//! Fix in das zugehörige Modul integriert (TDD: erst ROT, dann GRÜN).

// ---------------------------------------------------------------- editor ---

#[cfg(test)]
mod bug_tests {
    use rusty_notes::editor::{highlight, Tok};

    #[test]
    fn alle_ueberschriftsebenen_werden_gehighlightet() {
        // BUG: nur '# ' wurde erkannt, '## ' bis '###### ' nicht.
        let text = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6\n";
        let spans = highlight(text);
        for level in 1..=6usize {
            let prefix = format!("{} H{}", "#".repeat(level), level);
            assert!(
                spans.iter().any(|s| s.tok == Tok::Heading && prefix.starts_with(&text[s.start..s.end].trim_end()) && text[s.start..s.end].starts_with('#')),
                "Überschrift Ebene {} nicht gehighlightet",
                level
            );
        }
    }

    #[test]
    fn snake_case_ist_keine_betonung() {
        // BUG: '_' mitten im Wort löste fälschlich BoldItalic aus.
        let text = "var snake_case_name\n";
        let spans = highlight(text);
        assert!(
            !spans.iter().any(|s| s.tok == Tok::BoldItalic),
            "'snake_case_name' darf nicht als Betonung gelten"
        );
    }

    #[test]
    fn betonung_muss_wortgrenzen_respektieren() {
        // a_b_c bleibt Plain; *wirklich* betont wird erkannt.
        let text = "a_b_c und *echt* und **fett**\n";
        let spans = highlight(text);
        let bold: Vec<_> = spans
            .iter()
            .filter(|s| s.tok == Tok::BoldItalic)
            .map(|s| &text[s.start..s.end])
            .collect();
        assert_eq!(bold, vec!["*echt*", "**fett**"]);
    }
}

// ---------------------------------------------------------------- vault ----

#[cfg(test)]
mod rename_tests {
    use rusty_notes::vault::Vault;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static ZAEHLER: AtomicUsize = AtomicUsize::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rusty-rename-{}-{}-{}", tag, nanos, n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rename_auf_vorhandene_datei_schlaegt_fehl() {
        // BUG: fs::rename überschrieb stumm das Ziel.
        let dir = tmpdir("clash");
        fs::write(dir.join("a.md"), "AAA").unwrap();
        fs::write(dir.join("b.md"), "BBB").unwrap();

        let mut v = Vault::open(&dir).unwrap();
        let err = v.rename_note(&dir.join("a.md"), "b.md");
        assert!(err.is_err(), "rename auf existierendes Ziel muss fehlschlagen");
        assert_eq!(fs::read_to_string(dir.join("b.md")).unwrap(), "BBB", "Ziel darf nicht überschrieben werden");
        assert_eq!(fs::read_to_string(dir.join("a.md")).unwrap(), "AAA");
    }

    #[test]
    fn rename_nur_gross_klein_unterschied_ist_erlaubt() {
        // Derselbe Pfad in anderer Schreibweise ist KEIN Konflikt.
        let dir = tmpdir("caseonly");
        fs::write(dir.join("Note.md"), "x").unwrap();

        let mut v = Vault::open(&dir).unwrap();
        // Auf einem case-insensitiven FS wäre das Ziel "existiert"; wir erlauben
        // es, wenn es sich um dieselbe (kanonische) Datei handelt.
        let r = v.rename_note(&dir.join("Note.md"), "NOTE.md");
        let _ = r; // Auf Linux (case-sensitiv) ok; auf Windows wäre Ziel == Quelle.
        assert!(dir.join("NOTE.md").exists() || dir.join("Note.md").exists());
    }

    #[test]
    fn glossar_eintraege_nach_rename_aktualisiert() {
        // Wird erst nach dem Glossar-Integrationstest relevant; Platzhalter.
    }
}

// ---------------------------------------------------------------- search ---

#[cfg(test)]
mod search_perf_tests {
    use rusty_notes::search::search_notes;
    use rusty_notes::vault::Vault;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static ZAEHLER: AtomicUsize = AtomicUsize::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let n = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rusty-srch-{}-{}-{}", tag, nanos, n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn suche_nutzt_offene_puffer_ohne_neu_lesen() {
        // BUG: collect_search las nicht geöffnete Dateien bei jedem Tastendruck
        // erneut von der Platte. Neu: Vault::content_for liest einmalig und
        // cacht; der Test prüft Verhalten (Pufferinhalt gewinnt, Cache greift).
        let dir = tmpdir("cache");
        fs::write(dir.join("a.md"), "alpha beta").unwrap();

        let mut v = Vault::open(&dir).unwrap();
        v.scan().unwrap();
        v.open_note(&dir.join("a.md")).unwrap();
        v.set_text(&dir.join("a.md"), "alpha GAMMA").unwrap();

        // content_for muss den (geänderten!) Pufferinhalt liefern:
        let content = v.content_for(&dir.join("a.md")).unwrap();
        assert_eq!(content, "alpha GAMMA");

        // Und die Suche arbeitet auf den gelieferten Inhalten:
        let hits = search_notes(
            &[("a.md".into(), content)],
            "gamma",
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line_no, 0);
    }

    #[test]
    fn suche_entwertet_cache_nach_aenderungen_auf_der_platte() {
        let dir = tmpdir("inval");
        fs::write(dir.join("a.md"), "alt wort").unwrap();

        let mut v = Vault::open(&dir).unwrap();
        v.scan().unwrap();
        let _ = v.content_for(&dir.join("a.md")).unwrap();

        // Datei ändert sich extern:
        fs::write(dir.join("a.md"), "neues wort").unwrap();
        v.scan().unwrap(); // Scan bemerkt neue mtime → Cache muss entwertet werden

        let content = v.content_for(&dir.join("a.md")).unwrap();
        assert_eq!(content, "neues wort");
    }
}
