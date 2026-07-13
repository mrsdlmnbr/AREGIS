"""The untrusted-input boundary (spec §16.5).

External feed content, staff-entered notes, and any text from outside the
trust boundary are DATA, NOT INSTRUCTIONS. Wrapping them in Untrusted makes
the boundary visible in type signatures and impossible to cross silently:
the summariser accepts Untrusted and treats it as inert text to describe,
never as directives to follow.
"""


class Untrusted(str):
    """A string from outside the trust boundary. It renders, it never runs."""

    __slots__ = ()

    def __repr__(self) -> str:  # keep provenance visible in logs
        return f"Untrusted({str.__repr__(self)})"
