package config

import (
	"errors"
	"strings"
)

// Editor is a user-owned command, never inherited from a project or host.
type Editor struct {
	Command  []string `toml:"command"`
	Terminal *bool    `toml:"terminal,omitempty"`
}

func (e *Editor) Validate() error {
	if e == nil {
		return errors.New("configure [editor] in the global config with command = [\"nano\", \"{file}\"] and terminal = true")
	}
	if len(e.Command) < 2 || strings.TrimSpace(e.Command[0]) == "" {
		return errors.New("editor.command needs a program and one {file} argument")
	}
	files := 0
	for i, arg := range e.Command {
		if strings.ContainsRune(arg, 0) {
			return errors.New("editor.command must not contain NUL bytes")
		}
		if arg == "{file}" && i > 0 {
			files++
		} else if strings.ContainsAny(arg, "{}") {
			return errors.New("editor.command only supports {file} as a separate argument after the program")
		}
	}
	if files != 1 {
		return errors.New("editor.command needs exactly one {file} argument")
	}
	return nil
}

func (e *Editor) UsesTerminal() bool {
	return e == nil || e.Terminal == nil || *e.Terminal
}
