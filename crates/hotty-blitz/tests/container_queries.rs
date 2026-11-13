use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn render(markup: &str) -> Vec<u8> {
    let doc = format!(
        "<style>html,body{{margin:0;background:black;color:white;font:20px monospace}}</style>{markup}"
    );
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let cmd = |pairs: &[(&str, &str)], text: &str| {
        Message::new(pairs.iter().copied().collect(), text.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], &doc));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "4")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    host.frame("x").unwrap().rgba.clone()
}

#[test]
fn container_query_uses_its_laid_out_width() {
    let css = "<style>.box{container-type:inline-size;width:200px}@container (min-width:150px){span{color:red}}</style>";
    assert!(
        render(&format!("{css}<div class=box><span>A</span></div>"))
            == render("<div style=\"width:200px\"><span style=\"color:red\">A</span></div>")
    );
}

#[test]
fn nested_queries_settle_and_container_units_use_the_container() {
    let nested = "<style>.outer{container-type:inline-size;width:200px}.inner{container-type:inline-size;width:40px}@container (min-width:150px){.inner{width:100px}}@container (min-width:80px){span{color:red}}</style><div class=outer><div class=inner><span>A</span></div></div>";
    let expected = "<div style='width:200px'><div style='width:100px'><span style='color:red'>A</span></div></div>";
    assert!(render(nested) == render(expected));

    let units = "<style>.box{container-type:inline-size;width:200px}.bar{height:20px;width:50cqw;background:red}</style><div class=box><div class=bar></div></div>";
    let expected =
        "<div style='width:200px'><div style='height:20px;width:100px;background:red'></div></div>";
    assert!(render(units) == render(expected));
}

#[test]
fn size_container_does_not_claim_absolute_children() {
    let rules = "<style>.outer{position:relative}.box{margin-left:50px;width:100px;container-type:inline-size}.child{position:absolute;left:0;top:0;width:20px;height:20px;background:red}</style>";
    let actual =
        format!("{rules}<div class=outer><div class=box><div class=child></div></div></div>");
    let expected = "<div style='position:relative'><div style='margin-left:50px;width:100px'><div style='position:absolute;left:0;top:0;width:20px;height:20px;background:red'></div></div></div>";
    assert!(render(&actual) == render(expected));
}
