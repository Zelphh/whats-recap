use super::*;
use serde_json::json;
use std::cell::RefCell;
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
    let a = ask(db, q, None).unwrap();
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
fn list_messages() {
    with_db(|db| {
        let c = Conversation::open(db).unwrap();
        // Das 23h às 2h: 01:00, 01:02 e 01:03 de 01/01 e 23:00 de 02/01.
        let night = ListFilter {
            hour_from: Some(23),
            hour_to: Some(2),
            ..Default::default()
        };
        let r = c.execute(&ToolCall::ListMessages(night.clone())).unwrap();
        assert_eq!(
            r,
            ToolResult::ListMessages {
                total: 4,
                by_author: vec![2, 2],
                examples: vec![1, 2, 3, 6]
            }
        );
        let ids =
            |f: &ListFilter, limit| -> Vec<i64> { c.list_messages(f, limit).unwrap().iter().map(|m| m.id).collect() };
        assert_eq!(ids(&night, 2), [1, 2]);
        let te_amo = ListFilter {
            author: Some("ana".into()),
            word: Some("te amo".into()),
            ..Default::default()
        };
        assert_eq!(ids(&te_amo, 100), [3, 9]);
        // A mídia oculta entra na lista (não tem texto, mas é mensagem).
        let day = ListFilter {
            author: Some("Bruno".into()),
            from: NaiveDate::from_ymd_opt(2024, 3, 15),
            to: NaiveDate::from_ymd_opt(2024, 3, 15),
            ..Default::default()
        };
        assert_eq!(ids(&day, 100), [8]);
        assert!(matches!(
            c.execute(&ToolCall::ListMessages(ListFilter {
                hour_from: Some(25),
                ..Default::default()
            })),
            Err(ToolError::InvalidHour(25))
        ));

        let a = ask_ok(db, "quais foram as mensagens da Ana de madrugada?");
        assert_eq!(a.text, "Ana Souza mandou 2 mensagens da meia-noite às 6h.");
        assert_eq!(a.citations.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 3]);
        let a = ask_ok(db, "quais foram as mensagens entre 23h e 2h?");
        assert_eq!(a.text, "Foram 4 mensagens das 23h às 2h (Ana Souza: 2 · Bruno: 2).");
        let a = ask_ok(db, "mostra as mensagens com 'amor' de 2025");
        assert_eq!(a.text, "Não encontrei mensagens com “amor” em 2025.");
    });
}

#[test]
fn who_asks() {
    with_db(|db| {
        assert_eq!(me(db).unwrap(), None, "sem \"Você\" no export");
        let a = ask_ok(db, "quantas vezes ela falou 'amor'?");
        assert_eq!(a.tool.unwrap().author(), None);

        set_me(db, Some("Bruno")).unwrap();
        assert_eq!(me(db).unwrap().as_deref(), Some("Bruno"));
        let a = ask_ok(db, "quantas vezes ela falou 'amor'?");
        assert_eq!(a.tool.unwrap().author(), Some("Ana Souza"));
        assert!(a.text.starts_with("Ana Souza escreveu “amor” 1 vez"), "{}", a.text);
        let a = ask_ok(db, "quantas vezes eu falei 'amor'?");
        assert_eq!(a.tool.unwrap().author(), Some("Bruno"));

        assert!(matches!(
            set_me(db, Some("Carla")),
            Err(ToolError::UnknownAuthor { .. })
        ));
        set_me(db, None).unwrap();
        assert_eq!(me(db).unwrap(), None);
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

/// LLM de respostas fixas: uma para o roteador e outra para a escolha de ferramenta.
struct FakeLlm {
    route: Result<Value, String>,
    tool: Result<Value, String>,
    /// (prompt de sistema, pergunta, schema) de cada chamada.
    calls: RefCell<Vec<(String, String, Value)>>,
}

impl FakeLlm {
    fn new(route: &str, tool: Value) -> Self {
        FakeLlm {
            route: Ok(json!({ "type": route })),
            tool: Ok(tool),
            calls: RefCell::default(),
        }
    }
}

impl Llm for FakeLlm {
    fn complete(&self, system: &str, user: &str, schema: &Value, _: u32) -> Result<Value, String> {
        self.calls
            .borrow_mut()
            .push((system.to_string(), user.to_string(), schema.clone()));
        if system == llm::router_prompt() {
            self.route.clone()
        } else {
            self.tool.clone()
        }
    }
}

#[test]
fn llm_answers_what_the_rules_do_not_understand() {
    with_db(|db| {
        // O modelo preenche o período inteiro da conversa: equivale a não filtrar.
        let fake = FakeLlm::new(
            "quantitative",
            json!({ "tool": "messages_by_hour", "args": { "from": "2024-01-01", "to": "2025-02-10" } }),
        );
        let a = ask(db, "e aí, a gente conversa muito?", Some(&fake)).unwrap();
        assert_eq!(a.route, Route::Quantitative);
        assert_eq!(a.interpreted_by, Some(Interpreter::Llm));
        assert_eq!(
            a.tool,
            Some(ToolCall::MessagesByHour {
                author: None,
                from: None,
                to: None
            })
        );
        assert!(a.text.starts_with("Foram 10 mensagens"), "{}", a.text);

        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "roteador e ferramenta");
        let (system, user, schema) = &calls[1];
        assert_eq!(user, "e aí, a gente conversa muito?");
        assert!(
            system.contains("entre Ana Souza e Bruno (de 2024-01-01 a 2025-02-10)"),
            "{system}"
        );
        assert_eq!(schema, &llm::tool_schema(&["Ana Souza".into(), "Bruno".into()]));
    });
}

#[test]
fn rules_come_before_the_llm() {
    with_db(|db| {
        let fake = FakeLlm::new("global", json!({}));
        let a = ask(db, "Quantas vezes o Bruno falou 'amor'?", Some(&fake)).unwrap();
        assert_eq!(a.interpreted_by, Some(Interpreter::Heuristic));
        assert!(fake.calls.borrow().is_empty());
    });
}

#[test]
fn llm_without_a_fitting_tool() {
    with_db(|db| {
        let fake = FakeLlm::new("quantitative", json!({ "tool": "none", "args": {} }));
        let a = ask(db, "quantas vezes a gente brigou?", Some(&fake)).unwrap();
        assert!(a.tool.is_none());
        assert!(a.text.contains("não dá para responder com as contagens"), "{}", a.text);
        // O roteador por regras já decidiu (contagem): só a ferramenta passa pelo modelo.
        assert_eq!(fake.calls.borrow().len(), 1);
    });
}

#[test]
fn llm_routes_undecided_questions() {
    with_db(|db| {
        let fake = FakeLlm::new("global", json!({}));
        let a = ask(db, "oi, tudo bem?", Some(&fake)).unwrap();
        assert_eq!(a.route, Route::Global);
        assert!(a.tool.is_none());
    });
}

#[test]
fn llm_failure_falls_back_to_the_rules() {
    with_db(|db| {
        let fake = FakeLlm {
            route: Err("o servidor caiu".into()),
            tool: Err("o servidor caiu".into()),
            calls: RefCell::default(),
        };
        let a = ask(db, "oi, tudo bem?", Some(&fake)).unwrap();
        assert!(a.tool.is_none());
        assert!(
            a.text.contains("o servidor caiu") && a.text.contains("Experimente"),
            "{}",
            a.text
        );
    });
}

#[test]
fn intent_follows_the_question() {
    use intent::{Intent, intent_for};
    let by_hour = ToolCall::MessagesByHour {
        author: None,
        from: None,
        to: None,
    };
    assert_eq!(
        intent_for(&by_hour, "que horas a gente mais conversa?"),
        Intent::PeakHour
    );
    assert!(matches!(
        intent_for(&by_hour, "quem manda menos mensagem de manhã?"),
        Intent::WhoMost {
            most: false,
            hours: Some((6, 11, _))
        }
    ));
    let period = ToolCall::MessagesByPeriod {
        granularity: Granularity::Month,
        author: None,
        from: None,
        to: None,
    };
    assert_eq!(
        intent_for(&period, "mês com menos conversa"),
        Intent::TopPeriod { most: false }
    );
}
