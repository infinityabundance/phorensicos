// test_parser_simple.ph — a tiny expression parser simulation
// Tests: match on u64 tokens, recursion via loop, enum with data, chained if/else
package parser_simple;

enum Token {
    Num(u64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    End,
}

fn token_value(t: Token) -> u64 {
    let result: u64 = match t {
        Num => 1,
        Plus => 2,
        Minus => 3,
        Star => 4,
        Slash => 5,
        LParen => 6,
        RParen => 7,
        End => 0,
    };
    return result;
}

fn token_precedence(t: Token) -> u64 {
    let result: u64 = match t {
        Plus => 1,
        Minus => 1,
        Star => 2,
        Slash => 2,
        _ => 0,
    };
    return result;
}

fn is_operator(t: Token) -> bool {
    let val: u64 = token_value(t);
    return val >= 2 && val <= 5;
}

fn is_digit_char(c: u8) -> bool {
    return c >= 0x30 && c <= 0x39;
}

fn char_to_digit(c: u8) -> u64 {
    let val: u64 = c - 0x30;
    return val;
}

fn parse_simple_expr(tokens: Array(u64, 16)) -> u64 {
    let mut pos: u64 = 0;
    let mut result: u64 = 0;
    let mut current_op: u64 = 2;
    loop bound 16 {
        if pos >= 16 {
            return result;
        };
        let tok: u64 = tokens[pos];
        if tok == 0 {
            return result;
        };
        if current_op == 2 {
            result = result + tok;
        } else if current_op == 3 {
            result = result - tok;
        } else if current_op == 4 {
            result = result * tok;
        } else if current_op == 5 {
            result = result / tok;
        } else {
            result = result + tok;
        };
        pos = pos + 1;
        if pos >= 16 {
            return result;
        };
        current_op = tokens[pos];
        pos = pos + 1;
    };
    return result;
}

fn parse_and_eval(input: Array(u64, 16)) -> u64 {
    let mut i: u64 = 0;
    let mut nums: Array(u64, 8) = (0, 0, 0, 0, 0, 0, 0, 0);
    let mut num_count: u64 = 0;
    loop bound 16 {
        if i >= 16 {
            return parse_simple_expr(nums);
        };
        let tok: u64 = input[i];
        if tok == 0 {
            return parse_simple_expr(nums);
        };
        nums[num_count] = tok;
        num_count = num_count + 1;
        i = i + 1;
    };
    return parse_simple_expr(nums);
}

fn lex_simple(s: Array(u8, 16)) -> Array(u64, 16) {
    let mut tokens: Array(u64, 16) = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let mut pos: u64 = 0;
    let mut tok_idx: u64 = 0;
    loop bound 16 {
        if pos >= 16 {
            return tokens;
        };
        let c: u8 = s[pos];
        if c == 0 {
            return tokens;
        } else if c == 0x2B {
            tokens[tok_idx] = 2;
            tok_idx = tok_idx + 1;
        } else if c == 0x2D {
            tokens[tok_idx] = 3;
            tok_idx = tok_idx + 1;
        } else if c == 0x2A {
            tokens[tok_idx] = 4;
            tok_idx = tok_idx + 1;
        } else if c == 0x2F {
            tokens[tok_idx] = 5;
            tok_idx = tok_idx + 1;
        } else if is_digit_char(c) {
            tokens[tok_idx] = char_to_digit(c);
            tok_idx = tok_idx + 1;
        };
        pos = pos + 1;
    };
    return tokens;
}

fn main() -> u64 {
    let tokens: Array(u64, 16) = (3, 2, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let result: u64 = parse_simple_expr(tokens);
    let tokens2: Array(u64, 16) = (10, 4, 5, 3, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let result2: u64 = parse_simple_expr(tokens2);
    let tokens3: Array(u64, 16) = (8, 5, 4, 3, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let result3: u64 = parse_simple_expr(tokens3);
    let tv: u64 = token_value(Num);
    let prec: u64 = token_precedence(Plus);
    let is_op: bool = is_operator(Plus);
    return result + result2 + result3 + tv + prec;
}
