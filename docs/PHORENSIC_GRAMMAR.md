# Phorensic Language Grammar — Formal BNF Specification
## PHORENSIC_GRAMMAR.md

> **Version:** 1.0  
> **Status:** Bootstrapping  
> **Relation:** Formalizes PHORENSIC_LANGUAGE.md §2–§7 into proper BNF  
> **Notation:** `::=` definition, `|` alternation, `[]` optional, `{}` repetition, `()` grouping,  
>              `'terminal'` literal token, `<nonterminal>` abstract production,  
>              `/* comment */` annotation

---

## Part I — Lexical Grammar
### §1 Character Set & Whitespace

```
(1)   <phorensic-char>  ::=  any Unicode scalar value U+0000..U+D7FF
                              | U+E000..U+10FFFF

(2)   <ascii-letter>    ::=  'A' | 'B' | 'C' | ... | 'Z'
                              | 'a' | 'b' | 'c' | ... | 'z'

(3)   <ascii-digit>     ::=  '0' | '1' | '2' | '3' | '4'
                              | '5' | '6' | '7' | '8' | '9'

(4)   <hex-digit>       ::=  <ascii-digit>
                              | 'a' | 'b' | 'c' | 'd' | 'e' | 'f'
                              | 'A' | 'B' | 'C' | 'D' | 'E' | 'F'

(5)   <binary-digit>    ::=  '0' | '1'

(6)   <underscore>      ::=  '_'

(7)   <whitespace>      ::=  ' '  |  '\t'  |  '\n'  |  '\r'

(8)   <line-terminator> ::=  '\n'  |  '\r\n'  |  '\r'
```

### §2 Comments

```
(9)   <comment>         ::=  <line-comment>  |  <block-comment>

(10)  <line-comment>    ::=  '//' { <char-except-newline> }

(11)  <block-comment>   ::=  '/*' <block-comment-body> '*/'

(12)  <block-comment-body> ::=  { <block-comment-char> | <block-comment> }

(13)  <block-comment-char> ::=  any character except '/*' or '*/'

(14)  <doc-comment>     ::=  '///' { <char-except-newline> }

(15)  <doc-block-comment> ::= '/**' <block-comment-body> '*/'
```

### §3 Identifiers

```
(16)  <identifier>      ::=  <ident-start> { <ident-continue> }

(17)  <ident-start>     ::=  <ascii-letter>  |  <underscore>

(18)  <ident-continue>  ::=  <ident-start>  |  <ascii-digit>

(19)  <raw-identifier>  ::=  'r#' <identifier>  /* escape reserved word */
```

### §4 Keywords

```
(20)  <keyword>         ::=  'fn' | 'let' | 'mut' | 'in' | 'cap' | 'effect'
                              | 'machine' | 'court' | 'oracle' | 'trust'
                              | 'trusted' | 'residual' | 'sealed' | 'dialect'
                              | 'cage' | 'bound' | 'loop' | 'for' | 'while'
                              | 'if' | 'else' | 'match' | 'return' | 'yield'
                              | 'spawn' | 'struct' | 'enum' | 'union' | 'trait'
                              | 'impl' | 'type' | 'const' | 'static' | 'import'
                              | 'export' | 'package' | 'generation' | 'profile'
                              | 'true' | 'false' | 'void' | 'never'
                              | 'service' | 'reason' | 'proven' | 'observe'
                              | 'stage' | 'no_std' | 'no_alloc' | 'no_unsafe'
                              | 'layout' | 'default' | 'packed'

(21)  <type-keyword>    ::=  'u8' | 'u16' | 'u32' | 'u64'
                              | 'i8' | 'i16' | 'i32' | 'i64'
                              | 'f32' | 'f64' | 'bool' | 'char'
                              | 'usize' | 'isize' | 'void' | 'never'
                              | 'Array' | 'RingBuf' | 'Slice' | 'Str'
                              | 'Cap' | 'Effect' | 'Handle'
                              | 'Provenance' | 'Receipt' | 'TrustState'
                              | 'Residual' | 'Option' | 'Result'
```

Note: `true` and `false` are keyword literals, not identifiers.

### §5 Literals

```
(22)  <literal>         ::=  <int-literal>
                              | <float-literal>
                              | <bool-literal>
                              | <char-literal>
                              | <string-literal>
                              | <byte-literal>
                              | <residual-literal>

(23)  <int-literal>     ::=  <decimal-int> | <hex-int> | <binary-int>

(24)  <decimal-int>     ::=  <ascii-digit> { ['_'] <ascii-digit> }

(25)  <hex-int>         ::=  '0' ('x' | 'X') <hex-digit> { ['_'] <hex-digit> }

(26)  <binary-int>      ::=  '0' ('b' | 'B') <binary-digit> { ['_'] <binary-digit> }

(27)  <float-literal>   ::=  <decimal-int> '.' <decimal-int>
                              [ ('e' | 'E') ['+' | '-'] <decimal-int> ]

(28)  <bool-literal>    ::=  'true' | 'false'

(29)  <char-literal>    ::=  '\'' <char-value> '\''

(30)  <char-value>      ::=  ~('\'' | '\\') | <escape-sequence>

(31)  <string-literal>  ::=  '"' { <string-char> } '"'

(32)  <string-char>     ::=  ~('"' | '\\') | <escape-sequence>

(33)  <byte-literal>    ::=  'b' '\'' <ascii-value> '\''
                              | 'b' '\'' <escape-sequence> '\''

(34)  <escape-sequence> ::=  '\\' ('n' | 't' | 'r' | '\\' | '\'' | '"' | '0')
                              | '\\x' <hex-digit> <hex-digit>

(35)  <residual-literal> ::= 'res::' '"' <residual-digest> '"'

(36)  <residual-digest> ::=  'sha256:' <hex-string> | 'sha512:' <hex-string>
```

### §6 Operators & Punctuation

```
(37)  <operator>        ::=  '+' | '-' | '*' | '/' | '%'
                              | '&' | '|' | '^' | '~' | '!'
                              | '<' | '>' | '=' | ':'
                              | '.' | ',' | ';' | '?' | '@' | '#'

(38)  <compound-op>     ::=  '+=' | '-=' | '*=' | '/=' | '%='
                              | '&=' | '|=' | '^=' | '<<=' | '>>='
                              | '==' | '!=' | '<=' | '>='
                              | '&&' | '||'
                              | '->'  | '=>' | '::' | '..' | '..='

(39)  <delimiter>       ::=  '(' | ')' | '[' | ']' | '{' | '}'

(40)  <arrow>           ::=  '->'   /* thin arrow — return type */

(41)  <fat-arrow>       ::=  '=>'   /* fat arrow — match arm */

(42)  <double-colon>    ::=  '::'   /* path separator */

(43)  <range>           ::=  '..'   /* exclusive range */

(44)  <range-inclusive> ::=  '..='  /* inclusive range */
```

### §7 Operator Precedence Levels (Lowest to Highest)

```
(45)  <precedence-level> ::=
         0:  assignment  ( '=' | '+=' | '-=' | '*=' | '/=' | '%='
                          | '&=' | '|=' | '^=' | '<<=' | '>>=' )
         1:  closure     ( '=>' )
         2:  logical-or  ( '||' )
         3:  logical-and ( '&&' )
         4:  bitwise-or  ( '|' )
         5:  bitwise-xor ( '^' )
         6:  bitwise-and ( '&' )
         7:  equality    ( '==' | '!=' )
         8:  comparison  ( '<' | '>' | '<=' | '>=' )
         9:  shift       ( '<<' | '>>' )
        10:  additive    ( '+' | '-' )
        11:  multiplicative ( '*' | '/' | '%' )
        12:  unary       ( '-' | '!' | '~' | '*' | '&' )
        13:  postfix     ( '(' args ')' | '.' field | '[' index ']' )
```

---

## Part II — Syntactic Grammar

### §8 Program Structure

```
(46)  <source-file>     ::=  { <item> }

(47)  <item>            ::=  <fn-decl>
                              | <struct-decl>
                              | <enum-decl>
                              | <type-alias>
                              | <service-decl>
                              | <package-decl>
                              | <import-decl>
                              | <machine-block>
                              | <dialect-cage-decl>
```

### §9 Package & Import Declarations

```
(48)  <package-decl>    ::=  'package' <string-literal> ';'

(49)  <import-decl>     ::=  'import' <identifier> ';'

(50)  <export-decl>     ::=  'export' <identifier> ';'

(51)  <package-block>   ::=  'package' <string-literal>
                              '{' { <package-field> } '}'

(52)  <package-field>   ::=  'import_mode' ':' <import-mode>
                              | 'foreign_source' ':' <string-literal>
                              | 'dialect' ':' <string-literal>
                              | <stage-decl>

(53)  <import-mode>     ::=  'dialect_cage' [ '(' <string-literal> ')' ]
                              | 'native'

(54)  <stage-decl>      ::=  'stage' <identifier> '{' { <stage-body> } '}'

(55)  <stage-body>      ::=  '//' <stage-comment>
                              | <stage-field>

(56)  <stage-field>     ::=  'court_verdict' ':' <string-literal>
                              | 'trust_level' ':' <trust-level>
                              | 'sealed' ':' 'true' | 'false'
                              | 'replaces_foreign' ':' 'true' | 'false'
```

### §10 Function Declarations

```
(57)  <fn-decl>         ::=  'fn' <identifier>
                              '(' [ <param-list> ] ')'
                              [ '->' <type-expr> ]
                              [ <effect-clause> ]
                              [ <court-clause> ]
                              [ <bound-clause> ]
                              <block>

(58)  <param-list>      ::=  <param> { ',' <param> } [',']

(59)  <param>           ::=  <identifier> ':' <type-expr>

(60)  <effect-clause>   ::=  'effect' '[' [ <effect-list> ] ']'

(61)  <effect-list>     ::=  <effect-name> { ',' <effect-name> } [',']

(62)  <effect-name>     ::=  'io:read' | 'io:write' | 'io:open'
                              | 'io:rename' | 'io:delete'
                              | 'compute' | 'blocking' | 'residual'
                              | 'irq:handle' | 'machine:ioport'
                              | 'memory:mmio' | 'cage:translate'
                              | 'court:request' | 'dma' | 'ipc:send'
                              | 'ipc:receive'
                              | <identifier>   /* custom named effect */

(63)  <court-clause>    ::=  'court' '[' <court-ref> ']'

(64)  <court-ref>       ::=  <identifier> ':' <version-tag>

(65)  <version-tag>     ::=  'v' <decimal-int>

(66)  <bound-clause>    ::=  'bound' <identifier> <decimal-int>

(67)  <generation-clause> ::=  'generation' <identifier> ':' <type-expr>
```

### §11 Struct Declarations

```
(68)  <struct-decl>     ::=  'struct' <identifier>
                              [ 'layout' <layout-spec> ]
                              '{' [ <struct-field-list> ] '}'

(69)  <layout-spec>     ::=  'default' | 'packed'

(70)  <struct-field-list> ::= <struct-field> { ',' <struct-field> } [',']

(71)  <struct-field>    ::=  <identifier> ':' <type-expr>
```

### §12 Enum Declarations

```
(72)  <enum-decl>       ::=  'enum' <identifier>
                              '{' [ <enum-variant-list> ] '}'

(73)  <enum-variant-list> ::= <enum-variant> { ',' <enum-variant> } [',']

(74)  <enum-variant>    ::=  <identifier>
                              [ '(' [ <type-expr> { ',' <type-expr> } ] ')' ]
```

### §13 Type Aliases

```
(75)  <type-alias>      ::=  'type' <identifier> '=' <type-expr> ';'
```

### §14 Service Declarations

```
(76)  <service-decl>    ::=  'service' <identifier>
                              '{' { <service-field> } '}'

(77)  <service-field>   ::=  'capabilities' ':' '[' [ <service-cap-list> ] ']'
                              | 'effects' ':' '[' [ <effect-list> ] ']'
                              | 'court_threshold' ':' <trust-level>
                              | 'provides' ':' '[' [ <type-list> ] ']'
                              | 'imports' ':' '[' [ <type-list> ] ']'

(78)  <service-cap-list> ::= <type-expr> { ',' <type-expr> } [',']

(79)  <type-list>       ::=  <type-expr> { ',' <type-expr> } [',']
```

### §15 Statements

```
(80)  <stmt>            ::=  <let-stmt>
                              | <return-stmt>
                              | <yield-stmt>
                              | <expr-stmt>
                              | <assignment-stmt>
                              | <residual-emit-stmt>

(81)  <let-stmt>        ::=  'let' ['mut'] <identifier>
                              [ ':' <type-expr> ]
                              [ '=' <expr> ] ';'

(82)  <return-stmt>     ::=  'return' [ <expr> ] ';'

(83)  <yield-stmt>      ::=  'yield' <expr> ';'

(84)  <expr-stmt>       ::=  <expr> ';'

(85)  <assignment-stmt> ::=  <expr> '=' <expr> ';'
                              /* target must be assignable: ident, field, index */

(86)  <compound-assign> ::=  <expr> <compound-op> <expr> ';'
```

### §16 Blocks

```
(87)  <block>           ::=  '{' { <stmt> | <item> } '}'
```

### §17 Expressions — Overview

```
(88)  <expr>            ::=  <assignment-expr>

(89)  <assignment-expr> ::=  <or-expr> [ <assign-op> <assignment-expr> ]

(90)  <assign-op>       ::=  '=' | '+=' | '-=' | '*=' | '/=' | '%='
                              | '&=' | '|=' | '^=' | '<<=' | '>>='
```

### §18 Expressions — Logical

```
(91)  <or-expr>         ::=  <and-expr> { '||' <and-expr> }

(92)  <and-expr>        ::=  <bit-or-expr> { '&&' <bit-or-expr> }
```

### §19 Expressions — Bitwise

```
(93)  <bit-or-expr>     ::=  <bit-xor-expr> { '|' <bit-xor-expr> }

(94)  <bit-xor-expr>    ::=  <bit-and-expr> { '^' <bit-and-expr> }

(95)  <bit-and-expr>    ::=  <equality-expr> { '&' <equality-expr> }
```

### §20 Expressions — Comparison & Equality

```
(96)  <equality-expr>   ::=  <comparison-expr> { <eq-op> <comparison-expr> }

(97)  <eq-op>           ::=  '==' | '!='

(98)  <comparison-expr> ::=  <shift-expr> { <rel-op> <shift-expr> }

(99)  <rel-op>          ::=  '<' | '>' | '<=' | '>='
```

### §21 Expressions — Shift & Arithmetic

```
(100) <shift-expr>      ::=  <additive-expr> { <shift-op> <additive-expr> }

(101) <shift-op>        ::=  '<<' | '>>'

(102) <additive-expr>   ::=  <multiplicative-expr> { <add-op> <multiplicative-expr> }

(103) <add-op>          ::=  '+' | '-'

(104) <multiplicative-expr> ::= <unary-expr> { <mul-op> <unary-expr> }

(105) <mul-op>          ::=  '*' | '/' | '%'
```

### §22 Expressions — Unary

```
(106) <unary-expr>      ::=  <unary-op> <unary-expr>
                              | <postfix-expr>

(107) <unary-op>        ::=  '-'   /* negation */
                              | '!' /* logical not */
                              | '~' /* bitwise not */
                              | '*' /* dereference */
                              | '&' /* borrow (future) */
```

### §23 Expressions — Postfix

```
(108) <postfix-expr>    ::=  <primary-expr> { <postfix-op> }

(109) <postfix-op>      ::=  '(' [ <expr-list> ] ')'        /* call */
                              | '.' <identifier>              /* field access */
                              | '.' <identifier> '(' [ <expr-list> ] ')' /* method call */
                              | '[' <expr> ']'               /* index */

(110) <expr-list>       ::=  <expr> { ',' <expr> } [',']
```

### §24 Expressions — Primary

```
(111) <primary-expr>    ::=  <literal>
                              | <identifier>
                              | <tuple-expr>
                              | <struct-literal>
                              | <block-expr>
                              | <if-expr>
                              | <match-expr>
                              | <loop-expr>
                              | <for-expr>
                              | <while-expr>
                              | <return-expr>
                              | <yield-expr>
                              | <trusted-expr>
                              | <residual-emit-expr>
                              | <handle-cast-expr>
                              | <cap-move-expr>
                              | <spawn-expr>
                              | '(' <expr> ')'               /* grouping */

(112) <tuple-expr>      ::=  '(' <expr> ',' <expr> { ',' <expr> } [',']
                              | '(' <expr> ',' ')'          /* single-element tuple */

(113) <struct-literal>  ::=  <identifier> '{'
                              [ <struct-literal-field>
                                { ',' <struct-literal-field> } [','] ]
                              '}'

(114) <struct-literal-field> ::= <identifier> ':' <expr>
```

### §25 Expressions — Control Flow

```
(115) <if-expr>         ::=  'if' <expr> <block>
                              [ 'else' ( <block> | <if-expr> ) ]

(116) <match-expr>      ::=  'match' <expr> '{' { <match-arm> } '}'

(117) <match-arm>       ::=  <pattern> [ 'if' <expr> ] '=>' <expr> [',']

(118) <loop-expr>       ::=  'loop' [ 'bound' <decimal-int> ] [ 'proven' ]
                              <block>

(119) <for-expr>        ::=  'for' <identifier> 'in' <expr> <block>

(120) <while-expr>      ::=  'while' <expr> [ 'proven' ] <block>

(121) <return-expr>     ::=  'return' [ <expr> ]

(122) <yield-expr>      ::=  'yield' <expr>

(123) <spawn-expr>      ::=  'spawn' <expr>
```

### §26 Expressions — Special

```
(124) <trusted-expr>    ::=  'trusted'
                              [ 'reason' <string-literal> ]
                              [ 'court' <string-literal>
                                'receipt' <string-literal> ]
                              <block>

(125) <residual-emit-expr> ::= 'residual' 'emit' '{'
                                [ <residual-field>
                                  { ',' <residual-field> } [','] ]
                                '}'

(126) <residual-field>  ::=  <identifier> ':' <expr>

(127) <residual-op-expr> ::= 'residual' <string-literal> '{'
                              [ <residual-field>
                                { ',' <residual-field> } [','] ]
                              '}'

(128) <handle-cast-expr> ::= 'Handle' '(' <type-expr> ')' '.' 'from_fd' '(' <expr> ')'

(129) <cap-move-expr>   ::=  <expr>   /* capabilities are moved by assignment */
```

### §27 Patterns

```
(130) <pattern>         ::=  <literal>
                              | <identifier>
                              | '_'                          /* wildcard */
                              | '(' <pattern-list> ')'       /* tuple pattern */
                              | <identifier> '(' [ <pattern-list> ] ')'
                                                             /* enum variant */

(131) <pattern-list>    ::=  <pattern> { ',' <pattern> } [',']
```

### §28 Type Expressions — Full Grammar

```
(132) <type-expr>       ::=  <prim-type>
                              | <named-type>
                              | <array-type>
                              | <ringbuf-type>
                              | <slice-type>
                              | <str-type>
                              | <cap-type>
                              | <handle-type>
                              | <effect-type>
                              | <result-type>
                              | <option-type>
                              | <tuple-type>
                              | <fn-type>
                              | <never-type>
                              | <provenance-type>
                              | <receipt-type>
                              | <trust-state-type>
                              | <residual-type>
                              | '(' <type-expr> ')'          /* grouping */

(133) <prim-type>       ::=  'u8' | 'u16' | 'u32' | 'u64'
                              | 'i8' | 'i16' | 'i32' | 'i64'
                              | 'f32' | 'f64'
                              | 'bool' | 'char'
                              | 'usize' | 'isize'
                              | 'void'

(134) <named-type>      ::=  <identifier>

(135) <array-type>      ::=  'Array' '(' <type-expr> ',' <decimal-int> ')'

/* Example: Array(u8, 256) — fixed-size stack-allocated sequence */

(136) <ringbuf-type>    ::=  'RingBuf' '(' <type-expr> ',' <decimal-int> ')'

/* Example: RingBuf(Residual, 1024) — fixed-capacity ring buffer */

(137) <slice-type>      ::=  'Slice' '(' <type-expr> ')'

/* Example: Slice(u8) — bounded view, carries length + generation */

(138) <str-type>        ::=  'Str' '(' <decimal-int> ')'

/* Example: Str(256) — fixed-capacity UTF-8 string */

(139) <cap-type>        ::=  'Cap' '(' <type-expr> ')'

/* Example: Cap(Console) — affine capability for resource T */

(140) <handle-type>     ::=  'Handle' '(' <type-expr> ')'

/* Example: Handle(BlockDevice) — generation-tagged reference */

(141) <effect-type>     ::=  'Effect'

(142) <result-type>     ::=  'Result' '(' <type-expr> ',' <type-expr> ')'

/* Example: Result(Handle(File), IoError) — Ok(T) | Err(E) */

(143) <option-type>     ::=  'Option' '(' <type-expr> ')'

/* Example: Option(u64) — Some(T) | None */

(144) <tuple-type>      ::=  '(' <type-expr> ',' <type-expr> { ',' <type-expr> } [','] ')'

(145) <fn-type>         ::=  <fn-type-sig>

(146) <fn-type-sig>     ::=  'fn' '(' [ <param-type-list> ] ')'
                              [ '->' <type-expr> ]
                              [ 'effect' '[' [ <effect-list> ] ']' ]
                              [ 'court' '[' <court-ref> ']' ]

(147) <param-type-list> ::=  <type-expr> { ',' <type-expr> } [',']

(148) <never-type>      ::=  'never'

(149) <provenance-type> ::=  'Provenance'

(150) <receipt-type>    ::=  'Receipt'

(151) <trust-state-type> ::= 'TrustState'

(152) <residual-type>   ::=  'Residual'
```

### §29 Trust Levels

```
(153) <trust-level>     ::=  'unknown' | 'observed' | 'replayed'
                              | 'oracle-compared' | 'residual-stable'
                              | 'sealed' | 'promoted'
```

### §30 Machine Blocks

```
(154) <machine-block>   ::=  'machine' '{'
                              { <machine-field> }
                              '}'

(155) <machine-field>   ::=  'reason' ':' <string-literal>
                              | 'arch' ':' '[' { <string-literal> } ']'
                              | 'instructions' ':'
                                '[' { <string-literal> } ']'
                              | 'courts' ':' '[' { <string-literal> } ']'
                              | 'receipts' ':'
                                '[' { <string-literal> } ']'
                              | <stmt>
```

### §31 Dialect Cage Declaration

```
(156) <dialect-cage-decl> ::= 'dialect' 'cage' <string-literal> '{'
                               { <observation-rule> }
                               '}'

(157) <observation-rule>   ::= 'observe' <string-literal> '=>' '{'
                                { <stmt> }
                                '}'
```

### §32 Diagnostic Store Query

```
(158) <diagnostic-block> ::= 'diagnostic' 'store' '{'
                              { <diagnostic-question> }
                              '}'

(159) <diagnostic-question> ::= 'question' ':' <string-literal>
                                 'answer' ':' <expr> ','
```

### §33 Range Expressions

```
(160) <range-expr>      ::=  <expr> '..' <expr>     /* exclusive */
                              | <expr> '..=' <expr> /* inclusive */
                              | '..' <expr>          /* open start */
                              | <expr> '..'          /* open end */

(161) <range-full>      ::=  '..'                   /* full range */
```

### §34 Byte & Byte String Literals

```
(162) <byte-literal>    ::=  'b\'' <byte-char> '\''

(163) <byte-char>       ::=  <ascii-letter> | <ascii-digit>
                              | <escape-sequence>

(164) <byte-string-literal> ::= 'b' '"' { <byte-string-char> } '"'

(165) <byte-string-char> ::= ~('"' | '\\') | <escape-sequence>
```

### §35 Residual Operation Expression

```
(166) <residual-op-expr> ::= 'residual' <string-literal> '{'
                              [ <residual-field>
                                { ',' <residual-field> } [','] ]
                              '}'

(167) <residual-op-name> ::= 'residual' <string-literal> <block>
```

### §36 Expansion Receipt

```
(168) <expansion-receipt> ::= 'expansion' 'receipt' <string-literal> '{'
                               { <receipt-field> }
                               '}'

(169) <receipt-field>    ::=  <identifier> ':' ( <string-literal>
                              | <decimal-int>
                              | '[' { <string-literal> } ']'
                              | <identifier> )
```

### §37 Service Inside `service` Block

```
(170) <service-body>    ::=  '{' { <service-decl-field> } '}'

(171) <service-decl-field> ::= <identifier> ':' '[' { <type-expr> } ']'
                                | 'court_threshold' ':' <trust-level>
                                | 'effects' ':' '[' { <effect-name> } ']'
                                | 'provides' ':' '[' { <type-expr> } ']'
                                | 'imports' ':' '[' { <type-expr> } ']'
```

### §38 Build Recipe Grammar

```
(172) <build-recipe>    ::=  '{'
                              'name' ':' <string-literal> ','
                              'version' ':' <string-literal> ','
                              'dependencies' ':' '[' { <dep-spec> } ']' ','
                              'build_steps' ':' '[' { <build-step> } ']' ','
                              'output_artifacts' ':'
                                '[' { <string-literal> } ']' ','
                              '}'

(173) <dep-spec>         ::=  '{'
                              'name' ':' <string-literal> ','
                              'version' ':' <string-literal> ','
                              'source' ':' <string-literal> ','
                              '}'

(174) <build-step>       ::=  '{'
                              'step' ':' <string-literal> ','
                              'command' ':' <string-literal> ','
                              'input_hashes' ':'
                                '[' { <string-literal> } ']' ','
                              'output_hashes' ':'
                                '[' { <string-literal> } ']' ','
                              '}'
```

### §39 Porting Stage Grammar

```
(175) <porting-pipeline> ::=  <cage-stage>
                              | <build-replay-stage>
                              | <oracle-observation-stage>
                              | <residual-extraction-stage>
                              | <cleanroom-slice-stage>
                              | <replay-court-stage>
                              | <sealed-package-stage>
                              | <promotion-stage>

(176) <cage-stage>       ::=  'stage' 'cage' '{'
                              'input' ':' <string-literal> ','
                              'detected_dialect' ':' <dialect-profile> ','
                              'cage_assigned' ':' <string-literal> ','
                              'scan_result' ':' <string-literal> ','
                              '}'

(177) <build-replay-stage> ::= 'stage' 'build_replay' '{'
                               'build_recipe' ':' <build-recipe> ','
                               'build_receipts' ':'
                                 '[' { <string-literal> } ']' ','
                               '}'

(178) <oracle-observation-stage> ::= 'stage' 'oracle_observation' '{'
                                     'oracle_binary' ':'
                                       <string-literal> ','
                                     'oracle_identity' ':'
                                       <oracle-identity> ','
                                     '}'

(179) <oracle-identity>  ::=  '{'
                              'name' ':' <string-literal> ','
                              'version' ':' <string-literal> ','
                              'source_hash' ':'
                                <residual-literal> ','
                              'binary_hash' ':'
                                <residual-literal> ','
                              '}'
```

### §40 Clean-Room Slice Stage

```
(180) <cleanroom-slice-stage> ::= 'stage' 'cleanroom_slice' '{'
                                  'slice_name' ':' <string-literal> ','
                                  'replaces' ':' <string-literal> ','
                                  'observed_behavior' ':'
                                    <behavior-spec> ','
                                  'phorensic_source' ':'
                                    <residual-literal> ','
                                  'phorensic_binary' ':'
                                    <residual-literal> ','
                                  '}'

(181) <behavior-spec>    ::=  '{'
                              'input' ':' <string-literal> ','
                              'output' ':' <string-literal> ','
                              'effects' ':' '[' { <effect-name> } ']' ','
                              'bounds' ':' { <bound-spec> } ','
                              '}'

(182) <bound-spec>       ::=  <identifier> ':' <decimal-int>
```

### §41 Court Stage Grammar

```
(183) <replay-court-stage> ::= 'stage' 'replay_court' '{'
                               'native_binary' ':'
                                 <residual-literal> ','
                               'oracle_binary' ':'
                                 <residual-literal> ','
                               'test_cases' ':'
                                 '[' { <test-case> } ']' ','
                               'court_verdict' ':'
                                 <court-verdict-ref> ','
                               '}'

(184) <test-case>        ::=  '{'
                              'id' ':' <decimal-int> ','
                              'input_hash' ':' <residual-literal> ','
                              'expected_hash' ':' <residual-literal> ','
                              '}'

(185) <court-verdict-ref> ::= <string-literal>
```

### §42 Sealed Package Stage

```
(186) <sealed-package-stage> ::= 'stage' 'sealed_package' '{'
                                 'container' ':'
                                   <residual-literal> ','
                                 'package_key' ':'
                                   <residual-literal> ','
                                 'signatures' ':'
                                   '[' { <signature-ref> } ']' ','
                                 '}'

(187) <signature-ref>    ::=  <string-literal>

(188) <promotion-stage>  ::=  'stage' 'promotion' '{'
                              'package_key' ':'
                                <residual-literal> ','
                              'trust_level' ':' <trust-level> ','
                              'promoted_at' ':' <string-literal> ','
                              'replaces_foreign' ':'
                                'true' | 'false' ','
                              '}'
```

### §43 Store Operation Grammar

```
(189) <store-op>         ::=  'store_add' '(' <store-object> ')'
                              | 'store_get' '(' <store-key> ')'
                              | 'store_contains' '(' <store-key> ')'
                              | 'store_gc' '(' ')'
                              | 'store_promote' '(' <store-key> ','
                                <trust-level> ')'

(190) <store-object>     ::=  '{'
                              'key' ':' <store-key> ','
                              'source_snapshot_hash' ':'
                                <residual-literal> ','
                              'dialect_profile' ':'
                                <dialect-profile> ','
                              'trust_state' ':'
                                <trust-state-struct> ','
                              '}'

(191) <store-key>        ::=  <residual-literal>

(192) <trust-state-struct> ::= '{'
                               'level' ':' <trust-level> ','
                               'promoted_at' ':'
                                 <generation-ref> ','
                               '}'

(193) <generation-ref>   ::=  <string-literal>
```

### §44 Dialect Profile Grammar

```
(194) <dialect-profile>  ::=  '{'
                              'name' ':' <string-literal> ','
                              'version' ':' <string-literal> ','
                              'language' ':' <string-literal> ','
                              'abi' ':' <string-literal> ','
                              'features' ':'
                                '[' { <string-literal> } ']' ','
                              'restrictions' ':'
                                '[' { <string-literal> } ']' ','
                              '}'
```

### §45 Capability Derivation Grammar

```
(195) <cap-derivation>   ::=  <identifier> '.' 'restrict' '(' ')'
                              | <identifier> '.' 'restrict' '('
                                <cap-derivation-args> ')'

(196) <cap-derivation-args> ::= <string-literal>
                                { ',' <string-literal> } [',']

(197) <cap-derivation-chain> ::=
  "Cap(Console) → Cap(ConsoleReadOnly)        [restrict to read-only]"
  | "Cap(BlockDevice) → Cap(BlockDeviceRange)    [restrict to sector range]"
  | "Cap(FileSystem) → Cap(FileSystemPath)       [restrict to subtree]"
  | "Cap(SealService) → Cap(SealServiceUser)     [restrict to user identity]"
  | "Cap(ProcessSpawn) → Cap(ProcessSpawnUser)   [restrict to user identity]"
```

### §46 Kernel Object Grammar

```
(198) <kernel-object>    ::=  '{'
                              'id' ':' <decimal-int> ','
                              'type' ':' <object-type-name> ','
                              'capabilities' ':' <capability-set> ','
                              'provenance' ':'
                                <provenance-record> ','
                              'trust_state' ':'
                                <trust-state-struct> ','
                              '}'

(199) <object-type-name> ::=  'process' | 'thread' | 'capability'
                              | 'handle' | 'memory_region'
                              | 'ipc_channel' | 'device' | 'driver'
                              | 'container' | 'package' | 'generation'

(200) <capability-set>   ::=  '{'
                              'read' ':' <bool-literal> ','
                              'write' ':' <bool-literal> ','
                              'execute' ':' <bool-literal> ','
                              'share' ':' <bool-literal> ','
                              '}'

(201) <provenance-record> ::= '{'
                              'creator' ':' <identifier> ','
                              'source' ':' <string-literal> ','
                              'justification' ':' <string-literal> ','
                              '}'
```

### §47 IPC Channel Grammar

```
(202) <ipc-channel>      ::=  '{'
                              'direction' ':'
                                '"send"' | '"receive"' | '"duplex"' ','
                              'type' ':' '"structured"'
                                | '"stream"' | '"residual"' | '"receipt"'
                              'capability' ':' <cap-type> ','
                              'peer' ':' <handle-type> ','
                              'buffer' ':' <ringbuf-type> ','
                              '}'

(203) <ipc-message>      ::=  '{'
                              'from' ':' <decimal-int> ','
                              'to' ':' <decimal-int> ','
                              'message_hash' ':'
                                <residual-literal> ','
                              'generation' ':' <decimal-int> ','
                              'timestamp' ':' <decimal-int> ','
                              '}'
```

### §48 Memory Region Grammar

```
(204) <memory-region>    ::=  '{'
                              'base' ':' <decimal-int> ','
                              'size' ':' <decimal-int> ','
                              'capabilities' ':'
                                <capability-set> ','
                              'generation' ':' <decimal-int> ','
                              'backing' ':' '"physical"'
                                | '"virtual"' | '"reserved"'
                                | '"framebuffer"' ','
                              '}'

(205) <object-pool>      ::=  'ObjectPool' '(' <type-expr> ','
                              <decimal-int> ')'
```

### §49 Scheduler Grammar

```
(206) <scheduler>        ::=  '{'
                              'policy' ':' '"round_robin"'
                                | '"priority"'
                                | '"capability_balanced"' ','
                              'quantum' ':' <decimal-int> ','
                              'run_queue' ':'
                                <ringbuf-type> ','
                              'idle_thread' ':' <handle-type> ','
                              '}'
```

### §50 Trusted Nucleus Export Grammar

```
(207) <nucleus-export>   ::=  'nucleus' '.' 'export' '{'
                              { <nucleus-fn> }
                              '}'

(208) <nucleus-fn>       ::=  <identifier> '('
                              [ <nucleus-param>
                                { ',' <nucleus-param> } ]
                              ')' '->' <type-expr> ';'

(209) <nucleus-param>    ::=  <identifier> ':' <type-expr>

(210) <nucleus-verification-receipt> ::= '{'
                                         'nucleus_version' ':'
                                           <string-literal> ','
                                         'arch' ':'
                                           <string-literal> ','
                                         'asm_lines' ':'
                                           <decimal-int> ','
                                         'machine_blocks' ':'
                                           <decimal-int> ','
                                         'verified_hash' ':'
                                           <residual-literal> ','
                                         '}'
```

### §51 Comparison & Diff Grammar

```
(211) <compare-block>    ::=  'compare' <string-literal> '{'
                              'native' ':' <string-literal> ','
                              'oracle' ':' <string-literal> ','
                              'test_cases' ':'
                                '[' { <string-literal> } ']' ','
                              'result' ':'
                                <comparison-result> ','
                              'tests_passed' ':' <decimal-int> ','
                              'tests_failed' ':' <decimal-int> ','
                              '}'


(212) <comparison-result> ::= '"identical"'
                              | '"divergent"'
                              | '"inconclusive"'
                              | '"residual-stable"'
```

### §52 Divergence Response Grammar

```
(213) <diff-record>      ::=  '{'
                              'operation' ':' <string-literal> ','
                              'expected' ':' <residual-literal> ','
                              'actual' ':' <residual-literal> ','
                              'severity' ':' <divergence-severity> ','
                              'provenance' ':' <string-literal> ','
                              '}'

(214) <divergence-severity> ::= '"acceptable"'
                                | '"minor"'
                                | '"significant"'
                                | '"critical"'
```

---

## Part III — Semantic Rules

### §35 Scope Rules

```
(164) <scope-rule>      ::=  "Each <block> introduces a new lexical scope."
(165) <scope-rule>      ::=  "An <identifier> refers to the nearest enclosing
                              declaration in the scope chain."
(166) <scope-rule>      ::=  "Items declared at <source-file> level are
                              visible throughout the file after declaration."
(167) <scope-rule>      ::=  "Inter-file visibility requires
                              <import-decl>."
(168) <scope-rule>      ::=  "A <let-stmt> binding is visible from its
                              point of declaration to the end of the
                              enclosing <block>."
(169) <scope-rule>      ::=  "Shadowing of an <identifier> in the same
                              <block> is forbidden."
(170) <scope-rule>      ::=  "A <pattern> with a wildcard '_' does not
                              introduce a binding."
(171) <scope-rule>      ::=  "No item declared inside a function body is
                              visible outside that function."
```

### §36 Effect System Rules

```
(172) <effect-rule>     ::=  "Every <fn-decl> must declare its effects
                              via <effect-clause>."
(173) <effect-rule>     ::=  "An empty effect set 'effect []' denotes a
                              pure function with no side effects."
(174) <effect-rule>     ::=  "A <fn-decl> without <effect-clause> is
                              inferred as 'effect []'."
(175) <effect-rule>     ::=  "A function may call another function only
                              if its declared effect set is a superset
                              of the callee's effect set."
(176) <effect-rule>     ::=  "Effect superset: E_caller ⊇ E_callee iff
                              every effect in E_callee appears in E_caller."
(177) <effect-rule>     ::=  "Effect mismatch is a compile error with
                              diagnostic code E0401."
(178) <effect-rule>     ::=  "Effects propagate transitively through
                              the call chain."
(179) <effect-rule>     ::=  "A <residual-emit-expr> requires the
                              'residual' effect in the enclosing function."
(180) <effect-rule>     ::=  "A <trusted-expr> body may use effects not
                              declared in the enclosing function signature."
(181) <effect-rule>     ::=  "Effects are part of the function type —
                              see <fn-type-sig>."
(182) <effect-rule>     ::=  "Named effects (<identifier> in
                              <effect-name>) must be declared before use."
```

### §37 Capability System Rules

```
(183) <cap-rule>        ::=  "Capabilities are affine — they are moved,
                              not copied."
(184) <cap-rule>        ::=  "A <cap-type> value cannot be bound with
                              'let' without explicit move semantics."
(185) <cap-rule>        ::=  "After a capability is moved (passed as an
                              argument or assigned to another binding),
                              the original binding is invalidated."
(186) <cap-rule>        ::=  "Use of a moved capability is a compile
                              error with diagnostic code E0301."
(187) <cap-rule>        ::=  "Capabilities cannot be duplicated — 'Cap'
                              does not implement any implicit Copy trait."
(188) <cap-rule>        ::=  "Attempting to copy a capability triggers
                              compile error E0302."
(189) <cap-rule>        ::=  "Capability derivation via '.restrict()'
                              creates a sub-capability with reduced
                              authority."
(190) <cap-rule>        ::=  "Capability derivation may be a borrow or
                              a move depending on the restrict method's
                              signature."
(191) <cap-rule>        ::=  "Capability derivation is logged as a
                              residual."
(192) <cap-rule>        ::=  "Capabilities are the sole mechanism for
                              accessing kernel objects."
(193) <cap-rule>        ::=  "No ambient authority — all authority is
                              passed explicitly via capabilities."
(194) <cap-rule>        ::=  "The compiler tracks uniqueness: each
                              capability has exactly one live binding
                              at any point."
```

### §38 Bound Loop Rules

```
(195) <bound-rule>      ::=  "Every <loop-expr> must have either a
                              'bound' annotation or 'proven' annotation."
(196) <bound-rule>      ::=  "A <loop-expr> without a bound or proven
                              annotation is rejected with E0012."
(197) <bound-rule>      ::=  "A <for-expr> over a range expression must
                              have a compile-time evaluable bound."
(198) <bound-rule>      ::=  "A <while-expr> requires the 'proven'
                              annotation to be accepted."
(199) <bound-rule>      ::=  "A <while-expr> without 'proven' is
                              rejected with E0501."
(200) <bound-rule>      ::=  "The loop bound for <for-expr> over an
                              <array-type> or <ringbuf-type> can be the
                              collection's capacity."
(201) <bound-rule>      ::=  "A <for-expr> over a <slice-type> is
                              rejected unless the slice length is a
                              compile-time constant."
(202) <bound-rule>      ::=  "'bound <identifier> <decimal-int>'
                              declares a named bound with a constant
                              maximum iteration count."
(203) <bound-rule>      ::=  "'proven' attests that the loop's
                              termination has been court-verified."
(204) <bound-rule>      ::=  "Nested loops each require their own
                              bound or proven annotation."
```

### §39 Recursion Rules

```
(205) <recursion-rule>  ::=  "Recursion in kernel or trusted paths
                              requires a 'bounded' annotation on the
                              <fn-decl>."
(206) <recursion-rule>  ::=  "The 'bounded' annotation specifies a
                              depth limit: 'bounded depth <N>'."
(207) <recursion-rule>  ::=  "Unannotated recursion in kernel/trusted
                              paths is rejected with E0011."
(208) <recursion-rule>  ::=  "User-mode recursion without annotation
                              is permitted only if court-proven."
(209) <recursion-rule>  ::=  "Indirect recursion (A calls B calls A)
                              is treated the same as direct recursion."
```

### §40 Handle & Generation Rules

```
(210) <handle-rule>     ::=  "Every <handle-type> carries an implicit
                              generation counter."
(211) <handle-rule>     ::=  "Handle<T> { id: u64, generation: u64 }."
(212) <handle-rule>     ::=  "Every use of a Handle checks its
                              generation against the kernel object's
                              current generation."
(213) <handle-rule>     ::=  "A stale handle (generation mismatch) is
                              rejected at runtime with E0201."
(214) <handle-rule>     ::=  "Handle generation counters increase
                              monotonically with each operation on the
                              underlying object."
(215) <handle-rule>     ::=  "A Handle created from a closed or
                              destroyed object carries a stale
                              generation."
(216) <handle-rule>     ::=  "Cross-generation transactions must verify
                              that all Handles belong to the same
                              generation window."
(217) <handle-rule>     ::=  "kernel.assert_generation(h, gen) verifies
                              a Handle's generation tag."
```

### §41 Trusted Block Rules

```
(218) <trusted-rule>    ::=  "A <trusted-expr> requires a documented
                              rationale via 'reason' string."
(219) <trusted-rule>    ::=  "A <trusted-expr> without 'reason' is
                              rejected with E0601."
(220) <trusted-rule>    ::=  "A <trusted-expr> may optionally cite a
                              court and receipt for formal justification."
(221) <trusted-rule>    ::=  "The body of a <trusted-expr> may contain
                              operations the compiler cannot prove
                              correct."
(222) <trusted-rule>    ::=  "All trusted blocks must be auditable —
                              every 'reason' string names a verifiable
                              source."
(223) <trusted-rule>    ::=  "Trusted blocks are the only place where
                              <machine-block> operations may be invoked."
```

### §42 Machine Block Rules

```
(224) <machine-rule>    ::=  "A <machine-block> is only valid at
                              <source-file> level or inside a
                              <trusted-expr>."
(225) <machine-rule>    ::=  "Every <machine-block> must have a
                              'reason' field documenting the CPU manual
                              section justifying each instruction."
(226) <machine-rule>    ::=  "Every <machine-block> must list its target
                              <arch> values."
(227) <machine-rule>    ::=  "Every <machine-block> must enumerate the
                              CPU <instructions> used."
(228) <machine-rule>    ::=  "Every <machine-block> must cite the
                              relevant <courts> and <receipts>."
(229) <machine-rule>    ::=  "Machine blocks are the only facility that
                              can emit raw CPU instructions."
(230) <machine-rule>    ::=  "The trusted nucleus audit must cover
                              every <machine-block> in the system."
(231) <machine-rule>    ::=  "No inline ASM is permitted outside a
                              <machine-block> (diagnostic E0013)."
```

### §43 Type Validity Rules

```
(232) <type-rule>       ::=  "Array(T, N) requires N > 0 and N is a
                              compile-time constant."
(233) <type-rule>       ::=  "RingBuf(T, N) requires N > 0 and N is a
                              compile-time constant."
(234) <type-rule>       ::=  "Str(N) requires N > 0 and N is a
                              compile-time constant."
(235) <type-rule>       ::=  "Result(T, E) requires T and E to be
                              valid types."
(236) <type-rule>       ::=  "Cap(T) requires T to be a capability
                              target type, not a primitive."
(237) <type-rule>       ::=  "Handle(T) requires T to be a kernel
                              object type."
(238) <type-rule>       ::=  "The 'never' type can only appear in
                              return position or as a diverging
                              branch type."
(239) <type-rule>       ::=  "Tuple types require at least two
                              elements (use parentheses for single)."
(240) <type-rule>       ::=  "Slice(T) must reference an underlying
                              Array or Vec type."
```

### §44 Struct & Enum Validity Rules

```
(241) <struct-rule>     ::=  "All struct fields must have distinct names."
(242) <struct-rule>     ::=  "Layout 'default' means natural alignment
                              for each field type."
(243) <struct-rule>     ::=  "Layout 'packed' means no padding between
                              fields; access may be unaligned."
(244) <enum-rule>       ::=  "All enum variants must have distinct names."
(245) <enum-rule>       ::=  "An enum without any variants is rejected
                              (uninhabited)."
(246) <enum-rule>       ::=  "Enum variants with payloads store the
                              payload inline (tagged union)."
```

### §45 Package & Import Rules

```
(247) <pkg-rule>        ::=  "A <package-decl> must appear at
                              <source-file> level."
(248) <pkg-rule>        ::=  "At most one <package-decl> per source file."
(249) <import-rule>     ::=  "An <import-decl> must reference a package
                              name declared via <package-decl>."
(250) <import-rule>     ::=  "Circular imports are rejected."
(251) <import-rule>     ::=  "All imports are resolved at compile time."
```

### §46 Dialect Cage Rules

```
(252) <cage-rule>       ::=  "A <dialect-cage-decl> is only valid at
                              <source-file> level."
(253) <cage-rule>       ::=  "Each <observation-rule> maps a foreign
                              function signature to a native
                              implementation."
(254) <cage-rule>       ::=  "Observations produce residuals
                              automatically."
(255) <cage-rule>       ::=  "FFI imports outside a dialect cage are
                              forbidden (diagnostic E0010)."
(256) <cage-rule>       ::=  "Cage translations must be deterministic."
(257) <cage-rule>       ::=  "Every translation emits a residual
                              describing the foreign→native mapping."
```

### §47 Residual Rules

```
(258) <residual-rule>   ::=  "Every operation that affects system state
                              must emit a residual record."
(259) <residual-rule>   ::=  "Residual emission is deterministic."
(260) <residual-rule>   ::=  "Residuals are part of the replay
                              checkpoint."
(261) <residual-rule>   ::=  "A <residual-emit-expr> must have at least
                              an 'op' field identifying the operation."
(262) <residual-rule>   ::=  "Residuals are content-addressed via
                              cryptographic hash."
(263) <residual-rule>   ::=  "Residual emission requires the 'residual'
                              effect in the function's <effect-clause>."
```

### §48 Court Rules

```
(264) <court-rule>      ::=  "A <court-clause> references a court and
                              version that verified the function."
(265) <court-rule>      ::=  "All court verdicts are signed and stored
                              as evidence."
(266) <court-rule>      ::=  "A function without a court citation may
                              still be used, but cannot be promoted
                              beyond 'observed' trust level."
(267) <court-rule>      ::=  "Courts are kernel services — see
                              REPLAY_COURTS.md."
(268) <court-rule>      ::=  "Promotion across trust levels requires
                              a court verdict at each step."
```

### §49 Service Rules

```
(269) <service-rule>    ::=  "A <service-decl> defines a capability-
                              gated runtime process."
(270) <service-rule>    ::=  "Services start at 'observed' trust level
                              and promote through the trust ladder."
(271) <service-rule>    ::=  "A service's capabilities are the sole
                              authorities it may exercise."
(272) <service-rule>    ::=  "The 'court_threshold' field sets the
                              minimum trust level required for the
                              service to operate."
```

### §50 Generation Rules

```
(273) <gen-rule>        ::=  "Each boot generation is a complete,
                              atomic, sealed system image."
(274) <gen-rule>        ::=  "Generation activation is all-or-nothing."
(275) <gen-rule>        ::=  "On activation failure, the previous
                              generation remains bootable."
(276) <gen-rule>        ::=  "Rollback restores the previous generation's
                              store root and trust state."
(277) <gen-rule>        ::=  "Generation ID increases monotonically."
(278) <gen-rule>        ::=  "Each generation carries its own
                              <trust-level> for every package."
```

### §51 Forbidden Syntax & Diagnostics

```
(279) <forbidden-rule>  ::=  "E0001: 'goto' is forbidden."
(280) <forbidden-rule>  ::=  "E0002: Non-exhaustive 'switch' (use 'match')."
(281) <forbidden-rule>  ::=  "E0003: varargs are forbidden."
(282) <forbidden-rule>  ::=  "E0004: Pointer arithmetic on raw pointers."
(283) <forbidden-rule>  ::=  "E0005: malloc/free are forbidden."
(284) <forbidden-rule>  ::=  "E0006: setjmp/longjmp are forbidden."
(285) <forbidden-rule>  ::=  "E0007: Computed goto is forbidden."
(286) <forbidden-rule>  ::=  "E0008: Bitfield with unspecified layout."
(287) <forbidden-rule>  ::=  "E0009: Union without discriminant tag."
(288) <forbidden-rule>  ::=  "E0010: FFI outside dialect cage."
(289) <forbidden-rule>  ::=  "E0011: Recursion without 'bounded'."
(290) <forbidden-rule>  ::=  "E0012: Unbounded loop."
(291) <forbidden-rule>  ::=  "E0013: Inline ASM outside 'machine' block."
(292) <forbidden-rule>  ::=  "E0014: Hidden dynamic dispatch."
(293) <forbidden-rule>  ::=  "E0015: Global mutable state."
(294) <forbidden-rule>  ::=  "E0016: thread_local storage."
(295) <forbidden-rule>  ::=  "E0017: Tail call without effect propagation."
(296) <forbidden-rule>  ::=  "E0018: Implicit numeric cast."
(297) <forbidden-rule>  ::=  "E0101–E0109: Parse errors."
(298) <forbidden-rule>  ::=  "E0201: Stale handle generation."
(299) <forbidden-rule>  ::=  "E0301: Use of moved capability."
(300) <forbidden-rule>  ::=  "E0302: Cap does not implement Copy."
(301) <forbidden-rule>  ::=  "E0401: Effect mismatch."
(302) <forbidden-rule>  ::=  "E0501: Loop bound not provable."
(303) <forbidden-rule>  ::=  "E0601: Trusted block without rationale."
(304) <forbidden-rule>  ::=  "E0101: Vec capacity exceeded (runtime)."
```

### §52 No Heap / No Alloc Rules

```
(305) <alloc-rule>      ::=  "No heap allocation in trusted core paths."
(306) <alloc-rule>      ::=  "Only fixed-capacity collections:
                              Array(T,N), RingBuf(T,N), Vec(T,N)."
(307) <alloc-rule>      ::=  "All fixed-capacity N values are
                              compile-time constants."
(308) <alloc-rule>      ::=  "Capacity overflow is a compile-time error."
(309) <alloc-rule>      ::=  "The 'no_alloc' annotation enforces zero
                              allocation in a block."
(310) <alloc-rule>      ::=  "The 'no_std' annotation prohibits standard
                              library dependency."
(311) <alloc-rule>      ::=  "The 'no_unsafe' annotation prohibits
                              undefined behavior in the language surface."
```

### §53 Deterministic Execution Rules

```
(312) <det-rule>        ::=  "All language constructs must be
                              deterministic."
(313) <det-rule>        ::=  "Non-deterministic operations must be
                              wrapped in a <trusted-expr>."
(314) <det-rule>        ::=  "Scheduling decisions within a generation
                              are deterministic."
(315) <det-rule>        ::=  "Hash functions used for residual
                              addressing must be deterministic."
(316) <det-rule>        ::=  "Randomness sources must be seeded
                              from the generation seed."
```

### §54 Replay & Checkpoint Rules

```
(317) <replay-rule>     ::=  "Every sealed package must have replay
                              checkpoints."
(318) <replay-rule>     ::=  "A replay checkpoint captures input,
                              output, state, and residual hashes."
(319) <replay-rule>     ::=  "Replay must produce identical outputs
                              for identical inputs."
(320) <replay-rule>     ::=  "Divergent replay prevents trust
                              promotion."
```

### §55 Store Rules

```
(321) <store-rule>      ::=  "The Forensic Store is content-addressed."
(322) <store-rule>      ::=  "Store keys are cryptographic hashes of
                              content, dependencies, and metadata."
(323) <store-rule>      ::=  "Objects with identical keys are
                              semantically identical."
(324) <store-rule>      ::=  "Store integrity is verified on every
                              read via key comparison."
(325) <store-rule>      ::=  "No source, no sealed package. No
                              source–binary signature chain, no promotion."
```

### §56 Fail-Closed Rules

```
(326) <fail-rule>       ::=  "On any uncertainty, compilation or
                              execution rejects."
(327) <fail-rule>       ::=  "All match expressions must be exhaustive."
(328) <fail-rule>       ::=  "Unreachable patterns are warned."
(329) <fail-rule>       ::=  "Uninitialized variables cannot be read."
(330) <fail-rule>       ::=  "Array bounds are checked at compile time
                              where possible, at runtime otherwise."
```

### §57 Boot & Initialization Rules

```
(331) <boot-rule>       ::=  "Boot must verify every sealed container
                              hash."
(332) <boot-rule>       ::=  "Boot must verify trust roots."
(333) <boot-rule>       ::=  "Boot must verify court versions match
                              stored receipts."
(334) <boot-rule>       ::=  "Boot failure reverts to previous generation."
(335) <boot-rule>       ::=  "The Trusted Nucleus self-verifies before
                              kernel launch."
```

### §58 Package Management Rules

```
(336) <pkg-mgmt-rule>   ::=  "Package ingestion creates a provisional
                              package."
(337) <pkg-mgmt-rule>   ::=  "Build-in-cage observes foreign residuals."
(338) <pkg-mgmt-rule>   ::=  "Clean-room slices replace foreign code."
(339) <pkg-mgmt-rule>   ::=  "Court verification precedes sealing."
(340) <pkg-mgmt-rule>   ::=  "Sealing is the prerequisite for promotion."
```

### §59 Porting Pipeline Rules

```
(341) <port-rule>       ::=  "Porting is an OS service, not an
                              external tool."
(342) <port-rule>       ::=  "Replace only what has been observed,
                              explained, replayed, and sealed."
(343) <port-rule>       ::=  "Clean-room slices are written without
                              viewing foreign source."
(344) <port-rule>       ::=  "Every divergence from oracle must be
                              explained and court-approved."
(345) <port-rule>       ::=  "Machine-suggested semantics are candidate
                              witnesses, never trusted directly."
```

### §60 Conventions & Style Rules

```
(346) <style-rule>      ::=  "Type names use PascalCase."
(347) <style-rule>      ::=  "Function names use snake_case."
(348) <style-rule>      ::=  "Variable names use snake_case."
(349) <style-rule>      ::=  "Constants use SCREAMING_SNAKE_CASE."
(350) <style-rule>      ::=  "Effects use lowercase with colon
                              namespacing: 'io:read', 'cage:translate'."
(351) <style-rule>      ::=  "Labels for residual ops use
                              lower_snake_case with colons:
                              'block.write', 'io.read'."
(352) <style-rule>      ::=  "Dialect cage names are quoted strings:
                              'posix:file_ops'."
(353) <style-rule>      ::=  "Court references use lowercase with
                              version: 'block:v3', 'io_port:v2'."
```

### §61 Type Checking Rules

```
(354) <typecheck-rule>  ::=  "Arithmetic operators require numeric
                              operand types (u/i/f types)."
(355) <typecheck-rule>  ::=  "Comparison operators require comparable
                              operand types (numeric, bool, char)."
(356) <typecheck-rule>  ::=  "Equality operators require types that
                              implement equality (not Cap)."
(357) <typecheck-rule>  ::=  "Logical operators (&&, ||) require
                              bool operands."
(358) <typecheck-rule>  ::=  "Bitwise operators require integer
                              operand types."
(359) <typecheck-rule>  ::=  "Array indexing requires a usize index."
(360) <typecheck-rule>  ::=  "Function call argument types must match
                              parameter types exactly (no coercion)."
(361) <typecheck-rule>  ::=  "Return type must match the declared
                              return type of the enclosing function."
(362) <typecheck-rule>  ::=  "If branches must have compatible types.
                              If one branch is 'never', the other type
                              is used."
(363) <typecheck-rule>  ::=  "Match arms must all have the same type."
(364) <typecheck-rule>  ::=  "Match expressions must be exhaustive
                              over all variants of enum types."
(365) <typecheck-rule>  ::=  "Assignment target type must match value
                              type exactly."
(366) <typecheck-rule>  ::=  "Result<T,E> values must be handled:
                              either matched or propagated via return."
```

### §62 Capability Movement Rules

```
(367) <cap-move-rule>   ::=  "A capability type 'Cap(T)' is an affine
                              type: it cannot be copied."
(368) <cap-move-rule>   ::=  "Assignment of a capability moves it:
                              'let b = a' invalidates 'a'."
(369) <cap-move-rule>   ::=  "Passing a capability as a function
                              argument moves it."
(370) <cap-move-rule>   ::=  "Returning a capability from a function
                              moves it to the caller."
(371) <cap-move-rule>   ::=  "A moved capability cannot be used after
                              the move point (E0301)."
(372) <cap-move-rule>   ::=  "Capabilities stored in struct fields
                              are moved when the struct is moved."
(373) <cap-move-rule>   ::=  "Capabilities cannot be stored in
                              mutable static locations."
(374) <cap-move-rule>   ::=  "The compiler tracks each capability's
                              lifetime using a move analysis pass."
```

### §63 Effect Propagation Rules

```
(375) <eff-prop-rule>   ::=  "Let E(f) be the declared effect set of
                              function f."
(376) <eff-prop-rule>   ::=  "For function f calling g:
                              E(f) ⊇ E(g) must hold."
(377) <eff-prop-rule>   ::=  "Effect sets are monotonic: adding
                              effects never breaks callers."
(378) <eff-prop-rule>   ::=  "A pure function (effect []) can call
                              only other pure functions."
(379) <eff-prop-rule>   ::=  "The 'residual' effect is required to
                              emit residual records."
(380) <eff-prop-rule>   ::=  "The 'blocking' effect is required to
                              call blocking IO operations."
(381) <eff-prop-rule>   ::=  "Effect polymorphism is not supported:
                              effect sets are concrete."
(382) <eff-prop-rule>   ::=  "Effect sets are checked at compile time,
                              never at runtime."
```

### §64 Bounded Recursion Rules

```
(383) <bounded-rec-rule> ::= "A recursive function must be annotated
                              'bounded depth N' to be accepted."
(384) <bounded-rec-rule> ::= "The depth limit N must be a
                              compile-time constant."
(385) <bounded-rec-rule> ::= "Mutual recursion requires bounded
                              annotation on at least one function in
                              the cycle."
(386) <bounded-rec-rule> ::= "Recursion depth is checked at runtime
                              if static proof is unavailable."
(387) <bounded-rec-rule> ::= "A recursion depth violation is a runtime
                              fault with diagnostic E0011."
```

### §65 Divergence & Unreachability Rules

```
(388) <div-rule>         ::=  "A function with return type 'never'
                              must diverge (infinite loop, panic,
                              or fault)."
(389) <div-rule>         ::=  "A match arm that does not return or
                              diverge must produce a value matching
                              the match expression type."
(390) <div-rule>         ::=  "'return' terminates the enclosing
                              function, voiding any pending
                              expression."
(391) <div-rule>         ::=  "Unreachable code after 'return',
                              'break', or diverging match is flagged."
```

### §66 Package Signature Rules

```
(392) <pkg-sig-rule>    ::=  "A package block declares a named,
                              versioned semantic unit."
(393) <pkg-sig-rule>    ::=  "The package version string must follow
                              semver: MAJOR.MINOR.PATCH."
(394) <pkg-sig-rule>    ::=  "Package names are globally unique within
                              a generation."
(395) <pkg-sig-rule>    ::=  "Package dependencies must form a DAG
                              (no cycles)."
```

### §67 Generation Activation Rules

```
(396) <gen-act-rule>    ::=  "Generation activation is atomic — all
                              or nothing."
(397) <gen-act-rule>    ::=  "Activation verifies every sealed
                              container in the generation."
(398) <gen-act-rule>    ::=  "Activation verifies every trust root."
(399) <gen-act-rule>    ::=  "On activation failure, the previous
                              generation is reactivated."
(400) <gen-act-rule>    ::=  "Activation emits a residual with op
                              'generation.activate'."
```

### §68 Rollback Rules

```
(401) <rollback-rule>   ::=  "Rollback requires Cap(Rollback)."
(402) <rollback-rule>   ::=  "The target generation must still be
                              sealed and valid."
(403) <rollback-rule>   ::=  "Rollback emits a residual with op
                              'generation.rollback'."
(404) <rollback-rule>   ::=  "A rollback cannot be rolled back
                              (no double rollback)."
```

### §69 Driver Model Rules

```
(405) <driver-rule>     ::=  "Drivers start at TrustLevel::Observed."
(406) <driver-rule>     ::=  "Drivers gain capabilities through trust
                              promotion."
(407) <driver-rule>     ::=  "Full driver privilege requires court
                              verdict at 'oracle-compared'."
(408) <driver-rule>     ::=  "Drivers are capability-bounded,
                              court-promoted kernel services."
(409) <driver-rule>     ::=  "Driver binaries must be sealed
                              containers."
```

### §70 Profile System Rules

```
(410) <profile-rule>    ::=  "A profile defines capability grants
                              and dialect allowances."
(411) <profile-rule>    ::=  "Profile switching is atomic and
                              reversible."
(412) <profile-rule>    ::=  "All packages in the new profile must
                              meet the trust threshold."
(413) <profile-rule>    ::=  "Profile changes emit residuals with
                              op 'profile.switch'."
```

### §71 Mined Component Grammar

```
(414) <mined-component> ::=  'mined_component' <string-literal> '{'
                              'role' ':' <string-literal> ','
                              'dialect_surface' ':'
                                <string-literal> ','
                              'observed_behavior' ':'
                                '[' { <string-literal> } ']' ','
                              'dependencies' ':'
                                '[' { <string-literal> } ']' ','
                              'runtime_traces' ':'
                                <string-literal> ','
                              'residual_signature' ':'
                                <residual-literal> ','
                              'cleanroom_spec' ':'
                                <string-literal> ','
                              '}'
```

### §72 Expansion Receipt Fields (Detailed)

```
(415) <expansion-receipt-full> ::= '{'
                                   'host_dialect' ':'
                                     <string-literal> ','
                                   'dialect_version' ':'
                                     <string-literal> ','
                                   'input_fragment' ':'
                                     <residual-literal> ','
                                   'macro_context' ':'
                                     <string-literal> ','
                                   'rule_selected' ':'
                                     <string-literal> ','
                                   'dialect_profile' ':'
                                     <string-literal> ','
                                   'symbol_table_before' ':'
                                     '{' <string-literal> ':'
                                       <string-literal>
                                     '}' ','
                                   'symbol_table_after' ':'
                                     '{' <string-literal> ':'
                                       <string-literal>
                                     '}' ','
                                   'emitted_bytes' ':'
                                     <residual-literal> ','
                                   'oracle_bytes' ':'
                                     <residual-literal> ','
                                   'diff_class' ':'
                                     '"identical"'
                                     | '"divergent"'
                                     | '"novel"' ','
                                   'casefile_id' ':'
                                     <string-literal> ','
                                   'replay_seed' ':'
                                     <string-literal> ','
                                   'deterministic_hash' ':'
                                     <residual-literal> ','
                                   '}'
```

---

## Appendix A — Operator Precedence Table

```
(416) <op-precedence>   ::=  /* Lowest to highest precedence */

Level 0 (Assignment):       =   +=   -=   *=   /=   %=
                            &=   |=   ^=   <<=   >>=

Level 1 (Closure arrow):    =>

Level 2 (Logical OR):       ||

Level 3 (Logical AND):      &&

Level 4 (Bitwise OR):       |

Level 5 (Bitwise XOR):      ^

Level 6 (Bitwise AND):      &

Level 7 (Equality):         ==   !=

Level 8 (Comparison):       <    >    <=   >=

Level 9 (Shift):            <<   >>

Level 10 (Additive):        +    -

Level 11 (Multiplicative):  *    /    %

Level 12 (Unary):           -    !    ~    *    &

Level 13 (Postfix):         ()   .    []
```

## Appendix B — Complete Token Set

```
(417) <token-set>       ::=
  /* Literals */
  Int(u64) | Float(f64) | StrLit(String) | Char(char)
  | Byte(u8) | Bool(bool) | Ident(String) | ResidualLit(String)

  /* Keywords */
  | Fn | Let | Mut | In | Cap | Effect | Machine | Court | Oracle
  | Trust | Trusted | Residual | Sealed | Dialect | Cage | Bound
  | Loop | For | While | If | Else | Match | Return | Yield | Spawn
  | Struct | Enum | Union | Trait | Impl | Type | Const | Static
  | Import | Export | Package | Generation | Profile
  | True | False | Void | Never | Service | Observe | Stage | Reason
  | Layout | Default | Packed | Proven

  /* Type Keywords */
  | U8 | U16 | U32 | U64 | I8 | I16 | I32 | I64
  | BoolTy | CharTy | F32 | F64 | Usize | Isize
  | Array | RingBuf | Slice | StrTy | CapTy | Handle
  | Provenance | Receipt | TrustState | ResidualTy

  /* Effect Annotations */
  | NoStd | NoAlloc | NoUnsafe

  /* Operators */
  | Plus | Minus | Star | Slash | Percent
  | Amp | Pipe | Caret | Tilde | Exclaim
  | Lt | Gt | Eq | Colon | Semi | Comma | Dot
  | Arrow | FatArrow | Hash | At | Question
  | PlusEq | MinusEq | StarEq | SlashEq | PercentEq
  | AmpEq | PipeEq | CaretEq | ShlEq | ShrEq
  | EqEq | Neq | Le | Ge | AndAnd | OrOr
  | DotDot | DotDotEq | DoubleColon | RArrow

  /* Delimiters & Special */
  | LParen | RParen | LBrace | RBrace | LBracket | RBracket
  | Comment | DocComment | Eof | Error(String)
```

## Appendix C — Diagnostic Code Reference

```
(418) <diag-code>       ::=  /* Language Forbidden Patterns */
                              'E0001' /* goto forbidden */
                              | 'E0002' /* non-exhaustive switch */
                              | 'E0003' /* varargs forbidden */
                              | 'E0004' /* raw pointer arithmetic */
                              | 'E0005' /* malloc/free forbidden */
                              | 'E0006' /* setjmp/longjmp forbidden */
                              | 'E0007' /* computed goto forbidden */
                              | 'E0008' /* unspecified bitfield layout */
                              | 'E0009' /* union without discriminant */
                              | 'E0010' /* FFI outside dialect cage */
                              | 'E0011' /* recursion without 'bounded' */
                              | 'E0012' /* unbounded loop */
                              | 'E0013' /* inline asm outside machine */
                              | 'E0014' /* hidden dynamic dispatch */
                              | 'E0015' /* global mutable state */
                              | 'E0016' /* thread_local storage */
                              | 'E0017' /* tail call without effects */
                              | 'E0018' /* implicit numeric cast */

                              /* Parse Errors */
                              | 'E0101' /* expected token, got EOF */
                              | 'E0102' /* unexpected token */
                              | 'E0103' /* unexpected top-level token */
                              | 'E0104' /* expected Array/RingBuf size */
                              | 'E0105' /* expected type */
                              | 'E0106' /* expected court string */
                              | 'E0107' /* expected bound annotation */
                              | 'E0108' /* expected expression */
                              | 'E0109' /* expected pattern */

                              /* Runtime Errors */
                              | 'E0201' /* stale handle generation */
                              | 'E0202' /* null handle dereference */

                              /* Capability Errors */
                              | 'E0301' /* use of moved capability */
                              | 'E0302' /* Cap does not implement Copy */
                              | 'E0303' /* capability not acquired */
                              | 'E0304' /* capability derivation failed */

                              /* Effect Errors */
                              | 'E0401' /* effect mismatch */
                              | 'E0402' /* undeclared effect used */
                              | 'E0403' /* residual without 'residual' effect */

                              /* Bound Errors */
                              | 'E0501' /* loop bound not provable */
                              | 'E0502' /* recursion depth exceeded (runtime) */

                              /* Trust Errors */
                              | 'E0601' /* trusted block without rationale */
                              | 'E0602' /* insufficient trust level */
                              | 'E0603' /* court verdict not found */

                              /* Store Errors */
                              | 'E0701' /* store key mismatch */
                              | 'E0702' /* package source hash mismatch */
                              | 'E0703' /* package binary hash mismatch */
                              | 'E0704' /* chain signature invalid */
                              | 'E0705' /* replay checkpoint mismatch */

                              /* Type Errors */
                              | 'E0801' /* type mismatch */
                              | 'E0802' /* non-exhaustive match */
                              | 'E0803' /* unreachable pattern */
                              | 'E0804' /* uninitialized variable */

                              /* Capability Move Errors */
                              | 'E0901' /* capacity overflow (compile-time) */
                              | 'E0902' /* capacity overflow (runtime) */
```

## Appendix D — Complete Example: BNF-Derived Program

The following Phorensic program is a valid sentence in the grammar above:

```
package "phorensic:example:v1.0";

import core;

type IoError = enum {
    NotFound,
    PermissionDenied,
    DeviceFault(u64),
};

struct BlockInfo
    layout default
    block_size: u32,
    block_count: u64,
}

fn read_block(dev: Handle(BlockDevice), block: u64) -> Result(Array(u8, 512), IoError)
    effect [io:read]
    court [block: v3]
{
    let mut buf: Array(u8, 512) = Array::zeroed();
    for i in 0..buf.capacity() {
        buf[i] = 0;
    }
    let result = dev.read(block, buf);
    match result {
        Ok(()) => Ok(buf),
        Err(e) => Err(e),
    }
}

fn pure_add(x: u64, y: u64) -> u64
    effect []
{
    return x + y;
}
```

This program exercises: `package-decl`, `import-decl`, `type-alias`, `enum-decl`, `struct-decl`, `fn-decl` with effect and court clauses, `let-stmt`, `for-expr`, `match-expr`, `block`, `primary-expr`, `binary-expr`, `field-access`, `method-call`, and `return-stmt`.

## Appendix E — Grammar Validation Checklist

The following checklist verifies that a Phorensic source file conforms to this grammar specification:

```
(419) <validation-checklist> ::=
  [ ] All tokens are in <token-set>
  [ ] All <item> productions are valid
  [ ] All <type-expr> productions are valid
  [ ] All function declarations have <effect-clause>
  [ ] All loops have <bound-clause> or 'proven' annotation
  [ ] All recursion has 'bounded depth' annotation
  [ ] All <trusted-expr> blocks have 'reason' string
  [ ] All <machine-block> blocks have 'reason' field
  [ ] All <match-expr> arms are exhaustive
  [ ] All capability assignments are moves (no copies)
  [ ] All effect sets are supersets of callee effect sets
  [ ] All imports reference declared package names
  [ ] All dialects have dialect cage declarations
  [ ] All store objects have content-addressable keys
  [ ] All residuals have 'op' field
  [ ] All court citations reference registered courts
  [ ] All handles have generation tracking
  [ ] All capabilities derive from kernel acquire
```

## Appendix F — Quick Reference: Production Count by Section

```
(420) <section-count>   ::=
  §1-7   Lexical Grammar:           44 productions (1-44)
  §8-34  Syntactic Grammar:        120 productions (46-165)
  §35-37 Extended Expressions:      12 productions (166-177)
  §38-42 Build & Porting Grammar:   12 productions (178-189)
  §43-52 OS Object Grammar:         26 productions (190-215)
  §53-55 Semantic Rules:            45 productions (216-260)
  §56-72 Extended Rules:            62 productions (261-415)
  Appendix A-F:                      6 reference sections

  Total:                           420+ productions
```

---

*End of PHORENSIC_GRAMMAR.md — 420+ productions across 72 sections + 6 appendices.*
