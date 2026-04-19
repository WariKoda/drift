package config

import "sort"

// SortedHostsByName returns a copy of hosts sorted by name and hostname.
func SortedHostsByName(hosts []Host) []Host {
	result := append([]Host(nil), hosts...)
	sort.Slice(result, func(i, j int) bool {
		if result[i].Name != result[j].Name {
			return result[i].Name < result[j].Name
		}
		if result[i].Hostname != result[j].Hostname {
			return result[i].Hostname < result[j].Hostname
		}
		return result[i].Port < result[j].Port
	})
	return result
}
