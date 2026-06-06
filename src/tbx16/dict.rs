use crate::tbx16::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WordId(pub u16);

impl WordId {
    #[must_use]
    pub fn as_usize(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StringId(pub u16);

impl StringId {
    #[must_use]
    pub fn as_usize(self) -> usize {
        usize::from(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnMode {
    Void,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primitive {
    Dup,
    Drop,
    Swap,
    Over,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Negate,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Not,
    And,
    Or,
    BitAnd,
    BitOr,
    Fetch,
    Store,
    PutDec,
    PutChr,
    PutStr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instr {
    Call(WordId),
    Lit(Cell),
    Branch(usize),
    BranchIfZero(usize),
    BranchIfNonZero(usize),
    LoadLocal(u8),
    StoreLocal(u8),
    Exit,
    Halt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserWord {
    pub arity: u8,
    pub frame_slots: u8,
    pub return_mode: ReturnMode,
    pub code: Vec<Instr>,
}

impl UserWord {
    #[must_use]
    pub fn new(arity: u8, frame_slots: u8, return_mode: ReturnMode, code: Vec<Instr>) -> Self {
        assert!(
            frame_slots >= arity,
            "frame_slots must cover every argument slot"
        );
        Self {
            arity,
            frame_slots,
            return_mode,
            code,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Word {
    Primitive(Primitive),
    User(UserWord),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    words: Vec<Word>,
    strings: Vec<Vec<u8>>,
}

impl Program {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_word(&mut self, word: Word) -> Result<WordId, ProgramError> {
        let index = u16::try_from(self.words.len()).map_err(|_| ProgramError::TooManyWords)?;
        self.words.push(word);
        Ok(WordId(index))
    }

    pub fn add_string(&mut self, bytes: impl Into<Vec<u8>>) -> Result<StringId, ProgramError> {
        let index = u16::try_from(self.strings.len()).map_err(|_| ProgramError::TooManyStrings)?;
        self.strings.push(bytes.into());
        Ok(StringId(index))
    }

    #[must_use]
    pub fn word(&self, id: WordId) -> Option<&Word> {
        self.words.get(id.as_usize())
    }

    #[must_use]
    pub fn string(&self, id: StringId) -> Option<&[u8]> {
        self.strings.get(id.as_usize()).map(Vec::as_slice)
    }

    pub fn install_core_words(&mut self) -> Result<CoreWords, ProgramError> {
        Ok(CoreWords {
            dup: self.add_word(Word::Primitive(Primitive::Dup))?,
            drop: self.add_word(Word::Primitive(Primitive::Drop))?,
            swap: self.add_word(Word::Primitive(Primitive::Swap))?,
            over: self.add_word(Word::Primitive(Primitive::Over))?,
            add: self.add_word(Word::Primitive(Primitive::Add))?,
            sub: self.add_word(Word::Primitive(Primitive::Sub))?,
            mul: self.add_word(Word::Primitive(Primitive::Mul))?,
            div: self.add_word(Word::Primitive(Primitive::Div))?,
            modulo: self.add_word(Word::Primitive(Primitive::Mod))?,
            negate: self.add_word(Word::Primitive(Primitive::Negate))?,
            eq: self.add_word(Word::Primitive(Primitive::Eq))?,
            ne: self.add_word(Word::Primitive(Primitive::Ne))?,
            lt: self.add_word(Word::Primitive(Primitive::Lt))?,
            le: self.add_word(Word::Primitive(Primitive::Le))?,
            gt: self.add_word(Word::Primitive(Primitive::Gt))?,
            ge: self.add_word(Word::Primitive(Primitive::Ge))?,
            not: self.add_word(Word::Primitive(Primitive::Not))?,
            and: self.add_word(Word::Primitive(Primitive::And))?,
            or: self.add_word(Word::Primitive(Primitive::Or))?,
            bit_and: self.add_word(Word::Primitive(Primitive::BitAnd))?,
            bit_or: self.add_word(Word::Primitive(Primitive::BitOr))?,
            fetch: self.add_word(Word::Primitive(Primitive::Fetch))?,
            store: self.add_word(Word::Primitive(Primitive::Store))?,
            putdec: self.add_word(Word::Primitive(Primitive::PutDec))?,
            putchr: self.add_word(Word::Primitive(Primitive::PutChr))?,
            putstr: self.add_word(Word::Primitive(Primitive::PutStr))?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreWords {
    pub dup: WordId,
    pub drop: WordId,
    pub swap: WordId,
    pub over: WordId,
    pub add: WordId,
    pub sub: WordId,
    pub mul: WordId,
    pub div: WordId,
    pub modulo: WordId,
    pub negate: WordId,
    pub eq: WordId,
    pub ne: WordId,
    pub lt: WordId,
    pub le: WordId,
    pub gt: WordId,
    pub ge: WordId,
    pub not: WordId,
    pub and: WordId,
    pub or: WordId,
    pub bit_and: WordId,
    pub bit_or: WordId,
    pub fetch: WordId,
    pub store: WordId,
    pub putdec: WordId,
    pub putchr: WordId,
    pub putstr: WordId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramError {
    TooManyWords,
    TooManyStrings,
}
