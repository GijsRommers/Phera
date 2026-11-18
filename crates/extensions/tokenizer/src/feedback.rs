//! Parsed identifier feedback, e.g. `X::class` and `new X(if: 1)` use `T_STRING`.

use phera_lexer::TokenKind;

use crate::{ids::Id, stream::SourceToken, token};

#[derive(Clone, Copy, PartialEq)]
enum Block {
    Code,
    Class,
    Enum,
    TraitAdaptations,
}

impl Block {
    fn has_members(self) -> bool {
        matches!(self, Self::Class | Self::Enum)
    }
}

pub(crate) fn retag(tokens: &mut [SourceToken<'_>]) {
    use TokenKind::*;

    let significant: Vec<_> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !token::ignorable(token.id))
        .map(|(index, _)| index)
        .collect();
    let mut blocks = vec![Block::Code];
    let mut next_block = None;
    let mut constant_depth = None;
    let mut delimiter_depth = 0usize;
    let mut previous = Eof;
    let mut before_previous = Eof;

    for (i, &index) in significant.iter().enumerate() {
        let kind = tokens[index].kind;
        let next = significant
            .get(i + 1)
            .map_or(Eof, |&index| tokens[index].kind);
        let block = *blocks.last().unwrap_or(&Block::Code);
        let type_declaration = previous != DoubleColon
            && matches!(
                next,
                Identifier | Extends | Implements | LeftParen | LeftBrace
            );
        let trait_name = block == Block::TraitAdaptations
            && (matches!(previous, LeftBrace | Semicolon) && matches!(next, As | InsteadOf)
                || previous == As
                    && !matches!(before_previous, LeftBrace | Semicolon)
                    && !matches!(kind, Public | Protected | Private | Final)
                || before_previous == As
                    && matches!(previous, Public | Protected | Private | Final)
                    && next == Semicolon);
        let constant_name = constant_depth == Some(delimiter_depth) && next == Assign;
        let after_function = previous == Function
            || (matches!(previous, Ampersand | AmpersandBeforeVariable)
                && before_previous == Function);
        let is_name = is_keyword(&tokens[index])
            && (previous == DoubleColon
                || previous == Namespace
                || after_function && next == LeftParen
                || constant_name
                || trait_name
                || block == Block::Enum && previous == Case
                || matches!(previous, LeftParen | Comma) && next == Colon
                || kind == Readonly
                    && next == LeftParen
                    && (!block.has_members() || previous == Assign));
        if is_name {
            tokens[index].id = Id::String as i64;
        } else {
            match kind {
                Enum if type_declaration => next_block = Some(Block::Enum),
                Class | Trait | Interface if type_declaration => next_block = Some(Block::Class),
                Use if block.has_members() => next_block = Some(Block::TraitAdaptations),
                Const if block.has_members() => constant_depth = Some(delimiter_depth),
                _ => {}
            }
        }

        match kind {
            LeftBrace | CurlyOpen | DollarCurlyOpen => {
                blocks.push(next_block.take().unwrap_or(Block::Code));
                delimiter_depth += 1;
            }
            RightBrace => {
                blocks.pop();
                delimiter_depth = delimiter_depth.saturating_sub(1);
            }
            LeftParen | LeftBracket | AttributeStart => delimiter_depth += 1,
            RightParen | RightBracket => delimiter_depth = delimiter_depth.saturating_sub(1),
            Semicolon => {
                next_block = None;
                if constant_depth == Some(delimiter_depth) {
                    constant_depth = None;
                }
            }
            _ => {}
        }
        before_previous = previous;
        previous = if is_name { Identifier } else { kind };
    }
}

fn is_keyword(token: &SourceToken<'_>) -> bool {
    // Zend's semi_reserved production covers keywords and magic constants,
    // apart from yield-from, halt-compiler and asymmetric visibility.
    (Id::Include as i64..=Id::NsC as i64).contains(&token.id)
        && !matches!(
            token.kind,
            TokenKind::YieldFrom
                | TokenKind::HaltCompiler
                | TokenKind::PublicSet
                | TokenKind::ProtectedSet
                | TokenKind::PrivateSet
        )
}
