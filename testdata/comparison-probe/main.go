// Real Go comparison workflow, used as the Rust port's parity reference.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"time"

	"github.com/WariKoda/drift/internal/app"
	"github.com/WariKoda/drift/internal/config"
	"github.com/WariKoda/drift/internal/fs"
	"github.com/WariKoda/drift/internal/progress"
	syncpolicy "github.com/WariKoda/drift/internal/sync"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func run() error {
	var input struct {
		Local          []string          `json:"local"`
		Remote         []string          `json:"remote"`
		Mappings       []config.Mapping  `json:"mappings"`
		IncludeIgnored bool              `json:"include_ignored"`
		Sync           map[string]string `json:"sync"`
	}
	if err := json.NewDecoder(os.Stdin).Decode(&input); err != nil {
		return err
	}
	port, err := strconv.Atoi(os.Args[3])
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Join(os.Getenv("HOME"), ".ssh"), 0700); err != nil {
		return err
	}
	zero := 0
	protocol := "sftp"
	if len(os.Args) > 4 {
		protocol = os.Args[4]
	}
	host := config.Host{Name: "test", Hostname: "127.0.0.1", Port: port, User: "testuser", RootPath: os.Args[2], Protocol: protocol, Auth: config.Auth{Type: "password", Password: "test-password"}, Mappings: input.Mappings, KeepAliveInterval: &zero}
	local, remote := fs.NewSelectionState(), fs.NewSelectionState()
	for _, p := range input.Local {
		local.Marked[p] = struct{}{}
	}
	for _, p := range input.Remote {
		remote.Marked[p] = struct{}{}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	result, err := app.Load(ctx, app.LoadRequest{Host: host, Config: &config.MergedConfig{ProjectRoot: os.Args[1]}, Local: local, Remote: remote, Options: syncpolicy.ScopeOptions{IncludeIgnored: input.IncludeIgnored}}, progress.NewTracker("test"))
	if err != nil {
		return err
	}
	defer result.Root.Close()
	defer result.Conn.Close()
	if input.Sync != nil {
		items := make([]syncpolicy.Item, 0, len(result.Sessions))
		for _, session := range result.Sessions {
			name, err := filepath.Rel(os.Args[1], session.LocalPath)
			if err != nil {
				return err
			}
			decision := map[string]syncpolicy.Decision{"Upload": syncpolicy.DecisionUpload, "Download": syncpolicy.DecisionDownload, "Delete local": syncpolicy.DecisionDeleteLocal, "Delete remote": syncpolicy.DecisionDeleteRemote}[input.Sync[name]]
			items = append(items, syncpolicy.Item{LocalPath: session.LocalPath, RemotePath: session.RemotePath, Decision: decision})
		}
		synced := syncpolicy.Run(ctx, result.Conn, result.Root, items, progress.NewTracker("sync test"))
		if synced.Err != nil {
			return synced.Err
		}
		completed := make([]string, 0, len(synced.Completed))
		for _, index := range synced.Completed {
			name, err := filepath.Rel(os.Args[1], items[index].LocalPath)
			if err != nil {
				return err
			}
			completed = append(completed, name)
		}
		sort.Strings(completed)
		return json.NewEncoder(os.Stdout).Encode(struct {
			Completed []string `json:"completed"`
			Failures  int      `json:"failures"`
		}{completed, len(synced.Failures)})
	}
	type row struct {
		Local    string `json:"local"`
		Remote   string `json:"remote"`
		Status   string `json:"status"`
		Decision string `json:"decision"`
		Binary   bool   `json:"binary"`
	}
	rows := make([]row, 0, len(result.Sessions))
	for i := range result.Sessions {
		session := &result.Sessions[i]
		local, err := filepath.Rel(os.Args[1], session.LocalPath)
		if err != nil {
			return err
		}
		remote, err := filepath.Rel(os.Args[2], session.RemotePath)
		if err != nil {
			return err
		}
		status := "Changed"
		binary := false
		if session.Err != nil {
			status = "Error"
		} else if session.Result != nil {
			binary = session.Result.Binary
			if session.Result.LocalOnly {
				status = "Local only"
			}
			if session.Result.RemoteOnly {
				status = "Remote only"
			}
		}
		decision := map[syncpolicy.Decision]string{syncpolicy.DecisionNone: "Skip", syncpolicy.DecisionUpload: "Upload", syncpolicy.DecisionDownload: "Download"}[syncpolicy.AutoDecision(session)]
		rows = append(rows, row{local, remote, status, decision, binary})
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].Local < rows[j].Local })
	return json.NewEncoder(os.Stdout).Encode(rows)
}
