use super::*;
use std::sync::atomic::AtomicBool;

// Valores esperados, contados à mão:
// - "amor": Ana 1, Bruno 3 ("amooor" conta); "te amo": Ana 2 em 2024, Bruno 1 em 2025.
// - Madrugada (0h–5h): Ana 2, Bruno 1.
// - Janeiro de 2024: 6 mensagens (o mês com mais); 2024 tem 10 meses vazios (fevereiro e abril–dezembro).
// - Respostas (gap de 6h): Ana 60s, 600s, 240s (mediana 240s); Bruno 120s, 60s (mediana 90s).
const CHAT: &str = "\
01/01/2024 01:00 - Ana Souza: amor, você acordou?
01/01/2024 01:02 - Bruno: acordei amor
01/01/2024 01:03 - Ana Souza: te amo
01/01/2024 14:00 - Bruno: amooor vamos almoçar em Floripa?
01/01/2024 14:10 - Ana Souza: bora
02/01/2024 23:00 - Bruno: boa noite amor
15/03/2024 10:00 - Ana Souza: saudade da viagem
15/03/2024 10:01 - Bruno: <Mídia oculta>
15/03/2024 10:05 - Ana Souza: te amo muito
10/02/2025 09:00 - Bruno: te amo
";

fn with_db(f: impl FnOnce(&Db)) {
    let dir = std::env::temp_dir().join(format!(
        "wr-chat-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let txt = dir.join("chat.txt");
    std::fs::write(&txt, CHAT).unwrap();
    let db_path = dir.join("chat.db");
    let source = core_parser::Source::open(&txt).unwrap();
    core_storage::import_new(
        &db_path,
        &source,
        &Default::default(),
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    f(&Db::open(&db_path).unwrap());
    std::fs::remove_dir_all(dir).ok();
}

fn ask_ok(db: &Db, q: &str) -> ChatAnswer {
    let a = ask(db, q).unwrap();
    println!("{q}\n  → {}", a.text);
    a
}

#[test]
fn tools_directly() {
    with_db(|db| {
        let c = Conversation::open(db).unwrap();
        let r = c
            .execute(&ToolCall::CountWord {
                word: "amor".into(),
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::CountWord {
            total,
            by_author,
            messages,
            examples,
            ..
        } = r
        else {
            panic!()
        };
        assert_eq!((total, by_author, messages), (4, vec![1, 3], 4));
        assert_eq!(examples, vec![1, 2, 4, 6]);

        let r = c
            .execute(&ToolCall::MessagesByPeriod {
                granularity: Granularity::Month,
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::MessagesByPeriod {
            periods, top, bottom, ..
        } = r
        else {
            panic!()
        };
        assert_eq!(periods.len(), 14, "jan/2024 a fev/2025, com zeros");
        assert_eq!((top[0].period.as_str(), top[0].total), ("2024-01", 6));
        assert_eq!((bottom[0].period.as_str(), bottom[0].total), ("2024-02", 0));

        let r = c
            .execute(&ToolCall::MessagesByPeriod {
                granularity: Granularity::Weekday,
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::MessagesByPeriod { periods, .. } = r else {
            panic!()
        };
        // Segundas-feiras (0): 01/01/2024 (5 mensagens) e 10/02/2025 (1).
        assert_eq!(periods.iter().find(|p| p.period == "0").unwrap().total, 6);

        let r = c
            .execute(&ToolCall::MessagesByPeriod {
                granularity: Granularity::Week,
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::MessagesByPeriod { top, .. } = r else {
            panic!()
        };
        assert_eq!((top[0].period.as_str(), top[0].total), ("2024-01-01", 6));

        let r = c
            .execute(&ToolCall::ResponseTime {
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::ResponseTime { by_author, fastest } = r else {
            panic!()
        };
        assert_eq!((by_author[0].count, by_author[0].median_secs), (3, 240.0));
        assert_eq!((by_author[1].count, by_author[1].median_secs), (2, 90.0));
        assert_eq!(fastest, Some(1));

        let r = c
            .execute(&ToolCall::FirstOccurrence {
                word: "te amo".into(),
                author: Some("bruno".into()),
            })
            .unwrap();
        let ToolResult::FirstOccurrence { message: Some(m), .. } = r else {
            panic!()
        };
        assert_eq!(m.id, 10);

        assert!(matches!(
            c.execute(&ToolCall::MessagesByHour {
                author: Some("Carla".into()),
                from: None,
                to: None
            }),
            Err(ToolError::UnknownAuthor { .. })
        ));
    });
}

#[test]
fn questions_end_to_end() {
    with_db(|db| {
        let a = ask_ok(db, "Quantas vezes o Bruno falou 'amor'?");
        assert_eq!(a.text, "Bruno escreveu “amor” 3 vezes, em 3 mensagens.");
        assert_eq!(a.citations.len(), 3);
        assert_eq!(a.interpreted_by, Some(Interpreter::Heuristic));

        let a = ask_ok(db, "quantas vezes a Ana disse te amo em 2024?");
        assert_eq!(a.text, "Ana Souza escreveu “te amo” 2 vezes em 2024, em 2 mensagens.");

        let a = ask_ok(db, "quem fala mais 'amor'?");
        assert_eq!(
            a.text,
            "“amor” aparece 4 vezes (Ana Souza: 1 · Bruno: 3). Bruno usa mais."
        );

        let a = ask_ok(db, "Quem manda mais mensagem de madrugada?");
        assert_eq!(
            a.text,
            "De madrugada (0h–5h), Ana Souza manda mais mensagens: 2 contra 1 de Bruno."
        );

        let a = ask_ok(db, "Qual mês teve mais mensagens?");
        assert_eq!(a.text, "O mês com mais mensagens foi janeiro de 2024, com 6 mensagens.");

        let a = ask_ok(db, "qual mês teve menos mensagens em 2024?");
        assert_eq!(
            a.text,
            "10 meses em 2024 não tiveram nenhuma mensagem; o primeiro foi fevereiro de 2024."
        );
        let a = ask_ok(db, "qual ano teve menos mensagens?");
        assert_eq!(a.text, "O ano com menos mensagens foi 2025, com 1 mensagem.");

        let a = ask_ok(db, "Quem responde mais rápido?");
        assert_eq!(
            a.text,
            "Bruno responde mais rápido: mediana de 1min 30s, contra 4min de Ana Souza. (Médias: 1min 30s e 5min.)"
        );

        let a = ask_ok(db, "quanto tempo a Ana demora para responder?");
        assert_eq!(
            a.text,
            "Ana Souza leva em média 5min para responder (mediana de 4min, em 3 respostas)."
        );

        let a = ask_ok(db, "Quando foi a primeira vez que alguém disse \"te amo\"?");
        assert_eq!(
            a.text,
            "A primeira vez foi em 01/01/2024 às 01:03, por Ana Souza: “te amo”"
        );
        assert_eq!(a.citations[0].id, 3);

        let a = ask_ok(db, "que horas o Bruno mais manda mensagem?");
        assert!(
            a.text.starts_with("O horário com mais mensagens de Bruno é das "),
            "{}",
            a.text
        );

        let a = ask_ok(db, "quantas mensagens foram em março de 2024?");
        assert_eq!(a.text, "Foram 3 mensagens em março de 2024 (Ana Souza: 2 · Bruno: 1).");
    });
}

#[test]
fn other_routes() {
    with_db(|db| {
        let a = ask_ok(db, "como evoluiu a nossa relação?");
        assert_eq!(a.route, Route::Global);
        assert!(a.tool.is_none());

        let a = ask_ok(db, "o que a gente falou sobre a viagem para Floripa?");
        assert_eq!(a.route, Route::Retrieval);
        let ids: Vec<i64> = a.citations.iter().map(|m| m.id).collect();
        assert!(ids.contains(&4) && ids.contains(&7), "{ids:?}");

        let a = ask_ok(db, "oi, tudo bem?");
        assert_eq!(a.route, Route::Undecided);
        assert!(a.text.contains("Experimente"));

        let a = ask_ok(db, "quantas vezes a gente brigou?");
        assert_eq!(a.route, Route::Quantitative);
        assert!(a.tool.is_none());
    });
}

#[test]
fn examples_use_names() {
    let ex = examples(&["Ana Souza".into(), "Bruno".into()]);
    assert_eq!(ex[0], "Quantas vezes Bruno falou \"amor\"?");
    assert!(ex[4].contains("Ana disse"));
}
