"""The agent's tool surface: four read-only tools, frozen at import.

What this package deliberately does NOT have: credentials, a write path, or
any client of a service that can act. The registry refuses registration of
anything beyond the frozen four — an injected instruction cannot conjure a
fifth tool into existence.
"""

from dataclasses import dataclass, field
from typing import Callable, Dict, List, Optional, Tuple

from .untrusted import Untrusted

# The complete tool surface (spec §9.11). This tuple is the contract; the
# architecture test asserts the registry equals it exactly.
READ_ONLY_TOOL_NAMES = (
    "query_ontology",
    "search_events",
    "summarize_incident",
    "propose_coa",
)


@dataclass(frozen=True)
class CourseOfActionProposal:
    """A proposal a human must act on, or it does nothing (proto §10).

    requires_human is not a field with two values; construction with
    anything but True is an error. There is no other value.
    """

    alert_id: str
    suggested_rung: str
    rationale: str
    cited_event_ids: Tuple[str, ...]
    requires_human: bool = True

    def __post_init__(self) -> None:
        if self.requires_human is not True:
            raise ValueError(
                "requires_human is always True — the agent proposes, a human acts (spec §9.11)"
            )


@dataclass(frozen=True)
class Tool:
    name: str
    description: str
    fn: Callable


class ToolRegistry:
    """Frozen at construction. register() exists only to be refused."""

    def __init__(self, tools: List[Tool]) -> None:
        names = tuple(t.name for t in tools)
        if names != READ_ONLY_TOOL_NAMES:
            raise ValueError(
                f"the tool surface is closed: expected {READ_ONLY_TOOL_NAMES}, got {names}"
            )
        self._tools: Dict[str, Tool] = {t.name: t for t in tools}
        self.invocations: List[str] = []

    def names(self) -> Tuple[str, ...]:
        return tuple(self._tools.keys())

    def register(self, tool: "Tool") -> None:
        raise PermissionError(
            "the read-only tool surface is frozen (spec §9.11); a new tool is a spec change "
            "reviewed by humans, never a runtime registration"
        )

    def invoke(self, name: str, /, **kwargs):
        tool = self._tools.get(name)
        if tool is None:
            raise KeyError(f"no such tool {name!r} — the surface is {READ_ONLY_TOOL_NAMES}")
        self.invocations.append(name)
        return tool.fn(**kwargs)


# ── the four tool implementations (M0 stubs over the read-only Query API) ───

@dataclass(frozen=True)
class OntologyAnswer:
    answer: str
    cited_event_ids: Tuple[str, ...]
    uncertain: bool


def _query_ontology(property_id: str, query: str) -> OntologyAnswer:
    """Structured read-only query (M1 wires this to the bff's Query RPC —
    itself read-only). M0 returns an honest 'not connected'."""
    return OntologyAnswer(
        answer="ontology not connected in M0 sim scaffold",
        cited_event_ids=(),
        uncertain=True,
    )


def _search_events(property_id: str, text: str, limit: int = 20) -> List[str]:
    return []


def _summarize_incident(incident_text: Untrusted) -> OntologyAnswer:
    """Summarise text from OUTSIDE the trust boundary. The input is data:
    it is quoted, never followed. Note the type: refusing raw str keeps the
    boundary visible at every call site."""
    if not isinstance(incident_text, Untrusted):
        raise TypeError("summarize_incident takes Untrusted text — mark the boundary")
    snippet = str(incident_text)[:280]
    return OntologyAnswer(
        answer=f"[summary of untrusted content, quoted inert] {snippet!r}",
        cited_event_ids=(),
        uncertain=True,
    )


def _propose_coa(alert_id: str) -> CourseOfActionProposal:
    """Return a PROPOSAL object. Nothing here executes; a human reads it on
    the console and acts, or it evaporates."""
    return CourseOfActionProposal(
        alert_id=alert_id,
        suggested_rung="OBSERVE",
        rationale="M0 stub: observe first; 95% of the value is in rungs 1-2 (spec §3.1)",
        cited_event_ids=(),
    )


registry = ToolRegistry(
    [
        Tool("query_ontology", "read-only structured query over the twin", _query_ontology),
        Tool("search_events", "read-only text search over the event log", _search_events),
        Tool("summarize_incident", "summarise untrusted text as inert data", _summarize_incident),
        Tool("propose_coa", "produce a proposal a human must act on", _propose_coa),
    ]
)
