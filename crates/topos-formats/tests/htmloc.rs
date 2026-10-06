use topos_bible_formats::htmloc::*;

/**
For `1 Corinthians 12:27`: `members%20of%20it.-,1%20Corinthians%2012%3A27,-(ESV)`

URL: `https://svrbc.org/articles/2020-12-21/is-church-membership-biblical/#:~:text=members%20of%20it.-,1%20Corinthians%2012%3A27,-(ESV)`
*/
#[test]
fn test1() {
    let url = "https://svrbc.org/articles/2020-12-21/is-church-membership-biblical";
    let f = |tf: &TextFragment| format!("{url}{}", tf.to_hash_string());
    let html = include_str!("./fixtures/is-church-membership-biblical.html");
    let doc = Document::from_html(html);
    let doc = FragmentEngine::new(doc);

    /*
    1 Corinthians 12:27
    */
    let range = Selection {
        start: Position {
            line: 59,
            column: 25,
        },
        end: Position {
            line: 59,
            column: 44,
        },
        // Where the text is in the HTML (resolving the fragment must give it back)
        bytes: ByteIndex {
            start: 682253,
            end: 682272,
        },
    };

    let tf = doc.generate(range, None).unwrap();
    dbg!(&tf);
    println!("{}", f(&tf));
    assert_eq!(doc.resolve_fragment(&tf).unwrap(), range);

    let tf = doc
        .generate(range, Some(GenerateOptions::new(3, 3)))
        .unwrap();
    dbg!(&tf);
    println!("{}", f(&tf));
    assert_eq!(doc.resolve_fragment(&tf).unwrap(), range);
}

#[test]
fn test2() {
    let url = "https://svrbc.org/articles/2020-12-21/is-church-membership-biblical";
    let f = |tf: &TextFragment| format!("{url}{}", tf.to_hash_string());
    let html = include_str!("./fixtures/is-church-membership-biblical.html");
    let doc = Document::from_html(html);
    let doc = FragmentEngine::new(doc);
    /*
    He cannot pastor every
    Christian on the planet since he does not have the capacity to. He cannot
    pastor all who show up in a church on a given Sunday because the work of truly
    caring for someone requires a level of relationship that must extend beyond a
    mere day.
    */
    let range = Selection {
        start: Position {
            line: 78,
            column: 65,
        },
        end: Position {
            line: 82,
            column: 10,
        },
        // Where the text is in the HTML (resolving the fragment must give it back)
        bytes: ByteIndex {
            start: 683330,
            end: 683593,
        },
    };
    let tf = doc.generate(range, None).unwrap();
    dbg!(&tf);
    println!("{}", f(&tf));
    assert_eq!(doc.resolve_fragment(&tf).unwrap(), range);

    let tf = doc
        .generate(range, Some(GenerateOptions::new(3, 3)))
        .unwrap();
    dbg!(&tf);
    println!("{}", f(&tf));
    assert_eq!(doc.resolve_fragment(&tf).unwrap(), range);
}

#[test]
fn test3() {
    let url = "https://svrbc.org/articles/2020-12-21/is-church-membership-biblical";
    let f = |tf: &TextFragment| format!("{url}{}", tf.to_hash_string());
    let html = include_str!("./fixtures/is-church-membership-biblical.html");
    let doc = Document::from_html(html);
    let doc = FragmentEngine::new(doc);
    // https://svrbc.org/articles/2020-12-21/is-church-membership-biblical/#:~:text=The%20Responsibility%20of,it%20is%3A%20membership.
    /*
    The Responsibility of Pastors to Laymen
    The duty God assigns to pastors makes no practical sense apart from church membership.

    Pay careful attention to yourselves and to all the flock, in which the Holy Spirit has made you overseers, to care for the church of God, which he obtained with his own blood.

    Acts 20:28 (ESV)

    shepherd the flock of God that is among you, exercising oversight, not under compulsion, but willingly, as God would have you; not for shameful gain, but eagerly;

    1 Peter 5:2 (ESV)

    Who, precisely, is a pastor supposed to pastor? He cannot pastor every Christian on the planet since he does not have the capacity to. He cannot pastor all who show up in a church on a given Sunday because the work of truly caring for someone requires a level of relationship that must extend beyond a mere day. If he is to “exercise oversight,” he must have some means and authority to do so. For anyone who is not committed to a particular church, such an oversight would not only be difficult to conduct, but intrusive.

    Even the notion of a “flock of God that is among you” and “the flock, in which the Holy Spirit has made you overseers” refers to a particular congregation with particular borders. Whatever those borders are, they define membership.

    The Responsibility of Laymen to Pastors
    God instructs Christians to submit to elders. Once again, one cannot meaningfully fulfill this duty apart from membership in a local church.

    Obey your leaders and submit to them, for they are keeping watch over your souls, as those who will have to give an account. Let them do this with joy and not with groaning, for that would be of no advantage to you.

    Hebrews 13:17 (ESV)

    A Christian cannot, nor is required to, heed every pastor. No one has the time for such an endeavor, and the instruction of different pastors will vary, stripping the command of any real meaning. Additionally, if a Christian has the ability to immediately, at will, change which pastor is his pastor, then no real obligation can be found in this verse. For example, if I don’t like what my pastor says, I could just decide that today he will not be my pastor.

    The author of Hebrews clearly assumes that he addresses people who have entered into a committed relationship to each other by and through the local church.

    The Responsibility of Christians to Each Other
    The “one another”s in the New Testament have little shape outside of the commitment of church membership.

    Bear one another’s burdens, and so fulfill the law of Christ.

    Galatians 6:2 (ESV)

    Let the word of Christ dwell in you richly, teaching and admonishing one another in all wisdom, singing psalms and hymns and spiritual songs, with thankfulness in your hearts to God.

    Colossians 3:16 (ESV)

    Therefore encourage one another and build one another up, just as you are doing.

    1 Thessalonians 5:11 (ESV)

    Once again, a Christian cannot bear the burdens of all Christians, encourage all Christians, or sing to all Christians, etc. He could theoretically do these things with all Christians he encounters, but this would not match the kind of intimate fellowship we see in Scripture. Instead, there has to be some particular group one regularly engages with in these activities, and with some level of commitment. While people may disagree about that level of commitment, there is no reason not to call it what it is: membership.
    */
    let range = Selection {
        start: Position {
            line: 67,
            column: 2,
        },
        end: Position {
            line: 123,
            column: 58,
        },
        // Where the text is in the HTML (resolving the fragment must give it back)
        bytes: ByteIndex {
            start: 682775,
            end: 686175,
        },
    };
    let tf = doc.generate(range, None).unwrap();
    dbg!(&tf);
    // https://svrbc.org/articles/2020-12-21/is-church-membership-biblical#:~:text=assemblies.-,The%20Responsibility%20of,it%20is%3A%20membership.,-The
    println!("{}", f(&tf));
    assert_eq!(doc.resolve_fragment(&tf).unwrap(), range);

    let tf = doc
        .generate(range, Some(GenerateOptions::new(3, 3)))
        .unwrap();
    dbg!(&tf);
    // https://svrbc.org/articles/2020-12-21/is-church-membership-biblical#:~:text=in%20local%20assemblies.-,The%20Responsibility%20of,it%20is%3A%20membership.,-The%20Lord%E2%80%99s%20Supper
    println!("{}", f(&tf));
    // it includes
    assert_eq!(doc.resolve_fragment(&tf).unwrap(), range);
}

#[test]
fn reverse() {
    let html = include_str!("./fixtures/is-church-membership-biblical.html");
    let doc = Document::from_html(html);
    let doc = FragmentEngine::new(doc);
    // https://svrbc.org/articles/2020-12-21/is-church-membership-biblical/#:~:text=He%20cannot%20pastor%20every%20Christian%20on%20the%20planet%20since%20he%20does%20not%20have%20the%20capacity%20to.
    let hash = "#:~:text=He%20cannot%20pastor%20every%20Christian%20on%20the%20planet%20since%20he%20does%20not%20have%20the%20capacity%20to.";
    let tf = TextFragment::from_hash_string(hash).unwrap();
    let range = doc.resolve_fragment(&tf).unwrap();
    let tf2 = doc.generate(range, None).unwrap();
    // I don't expect hashes to be the same (my algorithm is different and I don't know what the
    // original is)
    // But the range round trip should be good
    let range2 = doc.resolve_fragment(&tf2).unwrap();
    assert_eq!(range, range2);
}
