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

// AutoDecision returns the default decision for a diff session. A file on one
// side only is copied to the other. A changed file is downloaded only when the
// remote copy is more than 2 seconds newer; otherwise, also when timestamps are
// missing, it is uploaded.
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
