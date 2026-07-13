"""AEGIS LLM copilot scaffold — READ-ONLY BY CONSTRUCTION (spec §9.11).

This is the component most likely to be built wrong, so the constraint is
structural, not procedural: the tool registry is frozen at import with
exactly four read-only tools, there are no credentials anywhere in this
package, no client of any mutating RPC exists here, and a course-of-action
proposal is an object a HUMAN must act on — it cannot execute.

A successful prompt injection therefore yields a wrong answer, not a moved
machine. That is a design property, not a filter. Do not add a filter and
call it safety; do not add a tool that acts and call it convenience.
"""

from .tools import (
    READ_ONLY_TOOL_NAMES,
    CourseOfActionProposal,
    Tool,
    ToolRegistry,
    registry,
)
from .untrusted import Untrusted

__all__ = [
    "READ_ONLY_TOOL_NAMES",
    "CourseOfActionProposal",
    "Tool",
    "ToolRegistry",
    "registry",
    "Untrusted",
]
