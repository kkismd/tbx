use tbx::cell::Cell;
use tbx::dict::WordEntry;
use tbx::expr::ExprCompiler;
use tbx::lexer::{Lexer, SpannedToken, Token};

fn lex(src: &str) -> Vec<SpannedToken> {
    let mut lx = Lexer::new(src);
    let mut out = Vec::new();
    loop {
        let st = lx.next_token();
        match &st.token {
            Token::Eof | Token::Newline => break,
            _ => out.push(st),
        }
    }
    out
}

#[test]
fn test_legacy_global_array_element_read_not_emitted() {
    let mut vm = tbx::init_vm();
    vm.dict_write(Cell::Int(0)).unwrap();
    vm.register(WordEntry::new_variable("A", 0));

    let array_get_xt = vm.lookup("ARRAY_GET").unwrap();

    let tokens = lex("A(2)");
    let result = ExprCompiler::new(&mut vm).compile_expr(&tokens);

    if let Ok(cells) = result {
        assert!(
            !cells.contains(&Cell::Xt(array_get_xt)),
            "A(i) must not emit ARRAY_GET: {cells:?}"
        );
    }
    // Compilation failure is also acceptable.
}
