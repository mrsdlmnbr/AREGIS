// ontologyd — serve the digital twin over gRPC from a file-backed event log.
//
//	ontologyd --property ridgeline --log /var/lib/aegis/world.log --listen :7200
//
// M1-dev entrypoint: the compose stack points bff and the console at this.
// Production runs the same server over the Redpanda-backed log with mTLS
// (spec §8); the serving code is identical either side of the seam.
package main

import (
	"flag"
	"fmt"
	"net"
	"os"
	"time"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/eventlog"
	"github.com/mrsdlmnbr/aregis/services/ontology/server"
	"google.golang.org/grpc"
)

func main() {
	property := flag.String("property", "", "property id this appliance serves")
	logPath := flag.String("log", "", "path to the append-only event log file")
	listen := flag.String("listen", ":7200", "gRPC listen address")
	flag.Parse()
	if *property == "" || *logPath == "" {
		fmt.Fprintln(os.Stderr, "ontologyd: --property and --log are required")
		os.Exit(2)
	}

	log, err := eventlog.OpenFileLog(*logPath)
	if err != nil {
		// A corrupt log refuses to serve rather than serving silently-wrong
		// state (I7 posture). The operator hears about it immediately.
		fmt.Fprintln(os.Stderr, "ontologyd:", err)
		os.Exit(1)
	}
	defer log.Close()

	// The single wall-clock injection point for this process (A7): domain
	// code below here receives time, never reads it.
	srv, err := server.New(*property, log, time.Now) // ARCHLINT-ALLOW: process edge
	if err != nil {
		fmt.Fprintln(os.Stderr, "ontologyd:", err)
		os.Exit(1)
	}

	lis, err := net.Listen("tcp", *listen)
	if err != nil {
		fmt.Fprintln(os.Stderr, "ontologyd:", err)
		os.Exit(1)
	}
	gs := grpc.NewServer()
	aegisv1.RegisterOntologyServiceServer(gs, srv)
	fmt.Printf("ontologyd: serving %s on %s from %s\n", *property, *listen, *logPath)
	if err := gs.Serve(lis); err != nil {
		fmt.Fprintln(os.Stderr, "ontologyd:", err)
		os.Exit(1)
	}
}
