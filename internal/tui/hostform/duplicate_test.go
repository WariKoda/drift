package hostform

import (
	"fmt"
	"reflect"
	"strings"
	"testing"

	"github.com/WariKoda/drift/internal/config"
	tea "github.com/charmbracelet/bubbletea"
)

func TestNewDuplicateCopiesIndependentFields(t *testing.T) {
	zero, interval := 0, 123
	for _, protocol := range []string{"sftp", "ftp", "ftps"} {
		for _, auth := range []config.Auth{
			{Type: "keyfile", KeyFile: "~/.ssh/deploy", Passphrase: "phrase"},
			{Type: "password", Password: "secret"},
			{Type: "agent"},
		} {
			if protocol != "sftp" && auth.Type != "password" {
				continue
			}
			for _, keepAlive := range []*int{nil, &zero, &interval} {
				for _, scope := range []config.HostScope{config.ScopeGlobal, config.ScopeProject} {
					t.Run(fmt.Sprintf("%s/%s/%v/%d", protocol, auth.Type, keepAlive, scope), func(t *testing.T) {
						h := config.Host{Name: "prod-copy", Hostname: "example.com", Port: 22, User: "deploy", RootPath: "/srv", Protocol: protocol, Auth: auth, KeepAliveInterval: keepAlive, Mappings: []config.Mapping{{Local: "src", Remote: "app"}}}
						m := NewDuplicate(h, scope, "shop", 100, 30)
						if m.isEdit || m.oldName != "" || m.scope != scope || !strings.Contains(m.View(), "Duplicate Host") {
							t.Fatal("incorrect duplicate form identity")
						}
						got, err := m.toHost()
						if err != nil {
							t.Fatal(err)
						}
						if !reflect.DeepEqual(got, h) {
							t.Fatal("copy did not preserve all host fields")
						}
						if keepAlive != nil {
							if got.KeepAliveInterval == keepAlive {
								t.Fatal("keep-alive pointer shared")
							}
							*got.KeepAliveInterval = 999
							if *keepAlive == 999 {
								t.Fatal("original keep-alive mutated")
							}
						}
						got.Mappings[0].Local = "output"
						if m.mappings[0].Local != "src" {
							t.Fatal("saved mappings share form storage")
						}
						m.mappings[0].Remote = "changed"
						m.fields[fHostname].SetValue("copy.example.com")
						m.fields[fKeepAliveInterval].SetValue("42")
						if h.Mappings[0].Remote != "app" || h.Hostname != "example.com" {
							t.Fatal("form mutated original")
						}
						other := NewDuplicate(h, scope, "shop", 100, 30)
						if other.fields[fHostname].Value() != "example.com" {
							t.Fatal("forms share fields")
						}
						_, cmd := other.Update(tea.KeyMsg{Type: tea.KeyCtrlS})
						if cmd == nil {
							t.Fatal("save returned no command")
						}
						saved, ok := cmd().(MsgHostSaved)
						if !ok || saved.OldName != "" || saved.Scope != scope || !reflect.DeepEqual(saved.Host, h) {
							t.Fatal("duplicate did not save as new host")
						}
					})
				}
			}
		}
	}
}
