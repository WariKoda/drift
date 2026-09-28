package hostform

import (
	"slices"
	"testing"

	"github.com/WariKoda/drift/internal/config"
)

func TestLinkFormEditsOnlyLinkFields(t *testing.T) {
	server := config.Host{Name: "kunde-x", Hostname: "kunde-x.de", Port: 22, User: "deploy", Auth: config.Auth{Type: "agent"}}
	m := NewLink(server, "prod", "/var/www", "shop", 120, 30)
	rows := m.visibleRows()
	if !slices.Equal(rows, []int{fName, fRootPath, fMappings}) {
		t.Fatalf("link form rows = %v", rows)
	}
	m.fields[fRootPath].SetValue("/var/www/shop")
	h, err := m.toHost()
	if err != nil {
		t.Fatal(err)
	}
	if h.Server != "kunde-x" || h.Hostname != "kunde-x.de" || h.RootPath != "/var/www/shop" || h.Name != "prod" {
		t.Fatalf("link = %+v", h)
	}
}
