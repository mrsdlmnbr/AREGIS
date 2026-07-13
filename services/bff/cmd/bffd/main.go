// bffd — the M1-dev edge: HTTP + SSE in front of ontologyd.
//
//	bffd --listen :7400 --ontology localhost:7200 --property ridgeline
//
// Dev transport only: plaintext gRPC upstream, persona from a header. The
// production edge terminates mTLS/OIDC and derives the persona from the
// credential — the route checks are identical (services/bff doctrine: the
// bff has no authority of its own).
package main

import (
	"flag"
	"fmt"
	"net/http"
	"os"

	aegisv1 "github.com/mrsdlmnbr/aregis/gen/go/aegis/v1"
	"github.com/mrsdlmnbr/aregis/services/bff"
	"github.com/mrsdlmnbr/aregis/services/bff/httpapi"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func main() {
	listen := flag.String("listen", ":7400", "HTTP listen address")
	ontology := flag.String("ontology", "localhost:7200", "ontologyd gRPC address")
	property := flag.String("property", "", "property id")
	flag.Parse()
	if *property == "" {
		fmt.Fprintln(os.Stderr, "bffd: --property is required")
		os.Exit(2)
	}

	conn, err := grpc.NewClient(*ontology, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		fmt.Fprintln(os.Stderr, "bffd:", err)
		os.Exit(1)
	}
	defer conn.Close()

	// Refuse to start on a route table that names a forbidden service —
	// the compiled-in floor of spec §9.13, checked again at every boot.
	routes := bff.DefaultRoutes()
	if violations := bff.Validate(routes); len(violations) != 0 {
		fmt.Fprintln(os.Stderr, "bffd: route table names a forbidden service:", violations)
		os.Exit(1)
	}

	srv := &httpapi.Server{
		Ontology: aegisv1.NewOntologyServiceClient(conn),
		Routes:   routes,
		Property: *property,
	}
	fmt.Printf("bffd: serving %s on %s (ontology at %s)\n", *property, *listen, *ontology)
	if err := http.ListenAndServe(*listen, srv.Handler()); err != nil {
		fmt.Fprintln(os.Stderr, "bffd:", err)
		os.Exit(1)
	}
}
