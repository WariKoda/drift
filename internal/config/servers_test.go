package config

import (
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func writeConfigFile(t *testing.T, path, text string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
		t.Fatal(err)
	}
}

func writeStore(t *testing.T, slug, text string) {
	t.Helper()
	path, err := projectStorePath(slug)
	if err != nil {
		t.Fatal(err)
	}
	writeConfigFile(t, path, text)
}

func readStore(t *testing.T, slug string) string {
	t.Helper()
	path, err := projectStorePath(slug)
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

const kundeServer = `
[[hosts]]
name = "kunde-x"
hostname = "kunde-x.de"
user = "deploy"
root_path = "/var/www"
[hosts.auth]
type = "password"
password = "$KUNDE_PW"
`

func TestLoadResolvesLinksAndHidesServers(t *testing.T) {
	isolate(t)
	writeConfigFile(t, globalConfigPath(), kundeServer)
	writeStore(t, "shop-b", `
[[hosts]]
name = "prod"
server = "kunde-x"
root_path = "/var/www/shop-b"
[[hosts.mappings]]
local = "public"
remote = "htdocs"
`)

	cfg, err := Load(t.TempDir(), "shop-b")
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := cfg.Hosts["kunde-x"]; ok {
		t.Fatal("a global server is a sync target of an open project")
	}
	got := cfg.Hosts["prod"]
	want := Host{
		Name: "prod", Server: "kunde-x", Hostname: "kunde-x.de", Port: 22, User: "deploy",
		Auth:     Auth{Type: "password", Password: "$KUNDE_PW"},
		RootPath: "/var/www/shop-b",
		Mappings: []Mapping{{Local: "public", Remote: "htdocs"}},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("resolved link = %+v, want %+v", got, want)
	}

	noProject, err := Load(t.TempDir(), "")
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := noProject.Hosts["kunde-x"]; !ok {
		t.Fatal("without a project the global hosts are not sync targets")
	}
}

func TestLoadRejectsBrokenLinks(t *testing.T) {
	for name, tc := range map[string]struct{ global, store, want string }{
		"connection field": {kundeServer, "[[hosts]]\nname = \"prod\"\nserver = \"kunde-x\"\nuser = \"other\"\n", "must not set"},
		"missing server":   {kundeServer, "[[hosts]]\nname = \"prod\"\nserver = \"gone\"\n", "not a global host"},
		"global link":      {"[[hosts]]\nname = \"a\"\nserver = \"b\"\n", "", "only valid for project hosts"},
	} {
		t.Run(name, func(t *testing.T) {
			isolate(t)
			writeConfigFile(t, globalConfigPath(), tc.global)
			if tc.store != "" {
				writeStore(t, "shop", tc.store)
			}
			_, err := Load(t.TempDir(), "shop")
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("Load error = %v, want %q", err, tc.want)
			}
		})
	}
}

func TestSavedLinkStoresOnlyItsOwnFields(t *testing.T) {
	isolate(t)
	writeConfigFile(t, globalConfigPath(), kundeServer)
	cfg, err := Load(t.TempDir(), "shop")
	if err != nil {
		t.Fatal(err)
	}
	link := Resolve(Host{Name: "prod", RootPath: "/var/www/shop"}, cfg.GlobalHosts[0])
	if err := SaveProjectHost(cfg, link, ""); err != nil {
		t.Fatal(err)
	}
	text := readStore(t, "shop")
	for _, leaked := range []string{"hostname", "password", "user", "port"} {
		if strings.Contains(text, leaked) {
			t.Fatalf("link record contains %s:\n%s", leaked, text)
		}
	}
	if !strings.Contains(text, `server = "kunde-x"`) {
		t.Fatalf("link record does not name its server:\n%s", text)
	}

	// Changing the server reaches the link without saving the link again.
	server := cfg.GlobalHosts[0]
	server.Hostname = "new.kunde-x.de"
	if err := SaveGlobalHost(cfg, server, "kunde-x"); err != nil {
		t.Fatal(err)
	}
	if got := cfg.Hosts["prod"].Hostname; got != "new.kunde-x.de" {
		t.Fatalf("link hostname after server edit = %q", got)
	}

	if err := SaveProjectHost(cfg, Host{Name: "broken", Server: "gone", RootPath: "/"}, ""); err == nil {
		t.Fatal("saved a link to a server that does not exist")
	}
}

func TestLinkedServerCannotBeDeletedOrRenamed(t *testing.T) {
	isolate(t)
	writeConfigFile(t, globalConfigPath(), kundeServer)
	writeStore(t, "shop-a", "[[hosts]]\nname = \"prod\"\nserver = \"kunde-x\"\nroot_path = \"/a\"\n")
	cfg, err := Load(t.TempDir(), "")
	if err != nil {
		t.Fatal(err)
	}

	var inUse *ServerInUseError
	if err := DeleteGlobalHost(cfg, "kunde-x"); !errors.As(err, &inUse) || !reflect.DeepEqual(inUse.Projects, []string{"shop-a"}) {
		t.Fatalf("delete of a linked server: %v", err)
	}
	renamed := cfg.GlobalHosts[0]
	renamed.Name = "kunde-y"
	if err := SaveGlobalHost(cfg, renamed, "kunde-x"); !errors.As(err, &inUse) {
		t.Fatalf("rename of a linked server: %v", err)
	}
	if _, ok := cfg.Hosts["kunde-x"]; !ok {
		t.Fatal("a refused change altered the session")
	}

	writeStore(t, "shop-a", "")
	if err := DeleteGlobalHost(cfg, "kunde-x"); err != nil {
		t.Fatalf("delete of an unused server: %v", err)
	}
}

func TestLinkTargetsAndMatches(t *testing.T) {
	isolate(t)
	writeConfigFile(t, globalConfigPath(), kundeServer)
	writeStore(t, "shop-a", `
[defaults]
user = "web"
[[hosts]]
name = "prod"
server = "kunde-x"
root_path = "/a"
[[hosts]]
name = "staging"
hostname = "staging.kunde-x.de"
protocol = "ftps"
root_path = "/stage"
`)
	writeStore(t, "shop-b", "[[hosts]]\nname = \"own\"\nhostname = \"own.example\"\nroot_path = \"/\"\n")
	cfg, err := Load(t.TempDir(), "shop-b")
	if err != nil {
		t.Fatal(err)
	}

	targets, err := LinkTargets(cfg)
	if err != nil {
		t.Fatal(err)
	}
	var got []string
	for _, target := range targets {
		got = append(got, target.Project+"/"+target.Host.Name+" "+strings.Join(target.UsedBy, ","))
	}
	want := []string{"/kunde-x shop-a", "shop-a/staging "}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("targets = %q, want %q", got, want)
	}
	if targets[1].Host.User != "web" || targets[1].Host.Port != 21 {
		t.Fatalf("project defaults not applied to target: %+v", targets[1].Host)
	}

	typed := Host{Hostname: "STAGING.kunde-x.de", Port: 21, User: "web", Protocol: "ftps"}
	if matches := MatchingTargets(targets, typed); len(matches) != 1 || matches[0].Host.Name != "staging" {
		t.Fatalf("matches = %+v", matches)
	}
	typed.User = "other"
	if matches := MatchingTargets(targets, typed); len(matches) != 0 {
		t.Fatalf("a different user matched: %+v", matches)
	}
}

func TestPromoteProjectHost(t *testing.T) {
	isolate(t)
	// A global host already has the name, so the server gets a prefixed one.
	writeConfigFile(t, globalConfigPath(), "[[hosts]]\nname = \"staging\"\nhostname = \"elsewhere\"\n")
	writeStore(t, "shop-a", `
[defaults]
user = "web"
[[hosts]]
name = "staging"
hostname = "staging.kunde-x.de"
root_path = "/stage"
[hosts.auth]
type = "keyfile"
key_file = "~/.ssh/kunde"
[[hosts.mappings]]
local = "src"
remote = "app"
`)
	cfg, err := Load(t.TempDir(), "shop-b")
	if err != nil {
		t.Fatal(err)
	}

	server, err := PromoteProjectHost(cfg, "shop-a", "staging")
	if err != nil {
		t.Fatal(err)
	}
	if server.Name != "shop-a-staging" || server.User != "web" || server.Auth.KeyFile != "~/.ssh/kunde" || server.Mappings != nil {
		t.Fatalf("server = %+v", server)
	}

	source, err := Load(t.TempDir(), "shop-a")
	if err != nil {
		t.Fatal(err)
	}
	got := source.Hosts["staging"]
	if got.Server != "shop-a-staging" || got.Hostname != "staging.kunde-x.de" || got.RootPath != "/stage" ||
		!reflect.DeepEqual(got.Mappings, []Mapping{{Local: "src", Remote: "app"}}) {
		t.Fatalf("source project after promotion = %+v", got)
	}
	if strings.Contains(readStore(t, "shop-a"), "key_file") {
		t.Fatal("source project still stores the credentials")
	}

	if _, err := PromoteProjectHost(cfg, "shop-a", "staging"); err == nil {
		t.Fatal("promoted a host that already is a link")
	}
}
