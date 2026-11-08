package diff

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"path/filepath"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/alecthomas/chroma/v2"
	"github.com/alecthomas/chroma/v2/lexers"
)

// SyntaxClass is palette-independent: changing the theme does not invalidate tokens.
type SyntaxClass uint8

const (
	SyntaxPlain SyntaxClass = iota
	SyntaxKeyword
	SyntaxString
	SyntaxNumber
	SyntaxComment
	SyntaxName
)

type SyntaxSpan struct {
	Start, End int
	Class      SyntaxClass
}

// SyntaxHighlights keeps both lexical contexts, even where the text is equal.
// Offsets refer to the original DiffLine.Text, never an ANSI-rendered string.
type SyntaxHighlights struct {
	Local, Remote map[int][]SyntaxSpan
}

// SyntaxCache retains just one bounded, immutable presentation result.
// Cancelled workers may overlap; expensive work never holds the lock.
type SyntaxCache struct {
	mu    sync.Mutex
	key   [32]byte
	valid bool
	value *SyntaxHighlights
}

const (
	syntaxMaxBytes     = 128 * 1024 // both reconstructed sides together
	syntaxMaxLineBytes = 2048
	syntaxMaxTokens    = 32000
	syntaxMaxLines     = 4000
	syntaxBudget       = 150 * time.Millisecond
)

// Highlight uses only the complete, normalized source already in the comparison.
// Unknown languages and resource-budget skips deliberately return plain text.
// The deadline is cooperative between lexer tokens, not a hard process timeout.
func (c *SyntaxCache) Highlight(ctx context.Context, name string, lines []DiffLine) (result *SyntaxHighlights, err error) {
	defer func() {
		if failure := recover(); failure != nil {
			result = nil
			err = fmt.Errorf("syntax lexer failed: %v", failure)
		}
	}()
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	lexer := lexers.Match(filepath.Base(name))
	if lexer == nil {
		return nil, nil
	}
	var sources [2]strings.Builder
	var indices [2][]int
	hash := sha256.New()
	hash.Write([]byte(lexer.Config().Name))
	bytes := 0
	for i, line := range lines {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if !utf8.ValidString(line.Text) || len(line.Text) > syntaxMaxLineBytes || strings.ContainsAny(line.Text, "\r\n") {
			return nil, nil
		}
		hash.Write([]byte{byte(line.Kind)})
		var size [8]byte
		binary.LittleEndian.PutUint64(size[:], uint64(len(line.Text)))
		hash.Write(size[:])
		hash.Write([]byte(line.Text))
		for side := range sources {
			if side == 0 && line.Kind == LineAdded || side == 1 && line.Kind == LineRemoved {
				continue
			}
			number := line.LocalNum
			if side == 1 {
				number = line.RemoteNum
			}
			if number != len(indices[side])+1 || len(indices[side]) >= syntaxMaxLines {
				return nil, nil
			}
			bytes += len(line.Text) + 1
			if bytes > syntaxMaxBytes {
				return nil, nil
			}
			sources[side].WriteString(line.Text)
			sources[side].WriteByte('\n')
			indices[side] = append(indices[side], i)
		}
	}
	var key [32]byte
	copy(key[:], hash.Sum(nil))
	c.mu.Lock()
	cached, hit := c.value, c.valid && c.key == key
	c.mu.Unlock()
	if hit {
		return cached, nil
	}
	budget, cancel := context.WithTimeout(ctx, syntaxBudget)
	defer cancel()
	result = &SyntaxHighlights{}
	for side := range sources {
		spans, tokenErr := tokenizeSyntax(budget, lexer, sources[side].String(), indices[side])
		if tokenErr != nil {
			if ctx.Err() != nil {
				return nil, ctx.Err()
			}
			if tokenErr == context.DeadlineExceeded {
				result = nil
				break
			}
			return nil, tokenErr
		}
		if side == 0 {
			result.Local = spans
		} else {
			result.Remote = spans
		}
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	c.key, c.value, c.valid = key, result, true
	return result, nil
}

func tokenizeSyntax(ctx context.Context, lexer chroma.Lexer, source string, indices []int) (map[int][]SyntaxSpan, error) {
	iterator, err := lexer.Tokenise(&chroma.TokeniseOptions{State: "root", EnsureLF: false}, source)
	if err != nil {
		return nil, err
	}
	spans := make(map[int][]SyntaxSpan)
	position, line, offset := 0, 0, 0
	for count := 0; ; count++ {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if count >= syntaxMaxTokens {
			return nil, context.DeadlineExceeded
		}
		token := iterator()
		if token == chroma.EOF {
			break
		}
		if !strings.HasPrefix(source[position:], token.Value) {
			return nil, fmt.Errorf("syntax lexer changed source text")
		}
		position += len(token.Value)
		class := SyntaxPlain
		switch {
		case token.Type.InCategory(chroma.Keyword):
			class = SyntaxKeyword
		case token.Type.InSubCategory(chroma.LiteralString):
			class = SyntaxString
		case token.Type.InSubCategory(chroma.LiteralNumber):
			class = SyntaxNumber
		case token.Type.InCategory(chroma.Comment):
			class = SyntaxComment
		case token.Type.InSubCategory(chroma.NameFunction), token.Type == chroma.NameClass:
			class = SyntaxName
		}
		for rest := token.Value; rest != ""; {
			end := strings.IndexByte(rest, '\n')
			if end < 0 {
				end = len(rest)
			}
			if line >= len(indices) {
				return nil, fmt.Errorf("syntax lexer exceeded source lines")
			}
			if end > 0 && class != SyntaxPlain {
				spans[indices[line]] = append(spans[indices[line]], SyntaxSpan{Start: offset, End: offset + end, Class: class})
			}
			offset += end
			if end == len(rest) {
				break
			}
			rest = rest[end+1:]
			line++
			offset = 0
		}
	}
	if position != len(source) {
		return nil, fmt.Errorf("syntax lexer omitted source text")
	}
	return spans, nil
}
