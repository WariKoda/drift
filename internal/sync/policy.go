package sync

import (
	"time"

	"github.com/WariKoda/drift/internal/diff"
)

// Decision represents a user-selectable sync action for a single file.
type Decision int

const (
	DecisionNone Decision = iota
	DecisionUpload
	DecisionDownload
	DecisionDeleteLocal
	DecisionDeleteRemote
)

// AutoDecision returns the most sensible default decision for a diff session.
//
// A 2-second threshold tolerates FAT32 time resolution and minor clock drift.
// When the files genuinely differ but the timestamps cannot disambiguate the
// direction — common over FTP/FTPS, where MDTM resolution is coarse and often
// unreliable — drift defaults to Upload rather than DecisionNone. Returning None
// here would silently drop the file from "sync all", which is the exact opposite
// of what a deploy-oriented tool should do.
func AutoDecision(s *diff.Session) Decision {
	if s == nil || s.Err != nil || s.Result == nil {
		return DecisionNone
	}

	r := s.Result
	switch {
	case r.LocalOnly:
		return DecisionUpload
	case r.RemoteOnly:
		return DecisionDownload
	case !r.HasDiff():
		return DecisionNone
	default:
		const threshold = 2 * time.Second
		delta := r.ModLocal.Sub(r.ModRemote)
		if delta < -threshold {
			return DecisionDownload
		}
		// Local newer, or timestamps too close to tell apart: upload.
		return DecisionUpload
	}
}

// NextDecision cycles through the valid decisions for a session state.
//
//	Both sides exist : None → Upload → Download → None
//	Local only       : None → Upload → DeleteLocal → None
//	Remote only      : None → Download → DeleteRemote → None
func NextDecision(cur Decision, s *diff.Session) Decision {
	if s == nil || s.Err != nil || s.Result == nil {
		return DecisionNone
	}

	switch {
	case s.Result.LocalOnly:
		switch cur {
		case DecisionNone:
			return DecisionUpload
		case DecisionUpload:
			return DecisionDeleteLocal
		default:
			return DecisionNone
		}
	case s.Result.RemoteOnly:
		switch cur {
		case DecisionNone:
			return DecisionDownload
		case DecisionDownload:
			return DecisionDeleteRemote
		default:
			return DecisionNone
		}
	default:
		switch cur {
		case DecisionNone:
			return DecisionUpload
		case DecisionUpload:
			return DecisionDownload
		default:
			return DecisionNone
		}
	}
}
