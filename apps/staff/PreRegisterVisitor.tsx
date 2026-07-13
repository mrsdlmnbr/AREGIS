// Pre-registration form — maps 1:1 to proto Expectation (the source of
// truth; do not add fields here that the proto does not have). Submitting
// calls OntologyService.UpsertExpectation via the bff under the staff
// member's RBAC. This form is the highest-leverage false-alarm feature in
// the product and it lives in the least glamorous app (spec §14.3).

import React, { useState } from "react";

export interface ExpectationDraft {
  person_id?: string; // enrolled person…
  vehicle_id?: string; // …or a known vehicle…
  onetime_code?: string; // …or a one-time gate code for a stranger
  zone_ids: string[];
  window_start: string; // ISO — time is an input everywhere (A7)
  window_end: string;
  rrule?: string; // "FREQ=WEEKLY;BYDAY=TU" for the housekeeper
  note?: string;
  registered_by: string;
}

export function PreRegisterVisitor({
  registeredBy,
  zones,
  onSubmit,
}: {
  registeredBy: string;
  zones: { id: string; name: string }[];
  onSubmit: (draft: ExpectationDraft) => void;
}) {
  const [draft, setDraft] = useState<ExpectationDraft>({
    zone_ids: [],
    window_start: "",
    window_end: "",
    registered_by: registeredBy,
  });
  const set = (patch: Partial<ExpectationDraft>) => setDraft((d) => ({ ...d, ...patch }));

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit(draft);
      }}
    >
      <label>
        Who — enrolled person id
        <input value={draft.person_id ?? ""} onChange={(e) => set({ person_id: e.target.value })} />
      </label>
      <label>
        …or vehicle plate
        <input value={draft.vehicle_id ?? ""} onChange={(e) => set({ vehicle_id: e.target.value })} />
      </label>
      <label>
        …or one-time gate code
        <input value={draft.onetime_code ?? ""} onChange={(e) => set({ onetime_code: e.target.value })} />
      </label>
      <fieldset>
        <legend>Zones</legend>
        {zones.map((z) => (
          <label key={z.id}>
            <input
              type="checkbox"
              checked={draft.zone_ids.includes(z.id)}
              onChange={(e) =>
                set({
                  zone_ids: e.target.checked
                    ? [...draft.zone_ids, z.id]
                    : draft.zone_ids.filter((id) => id !== z.id),
                })
              }
            />
            {z.name}
          </label>
        ))}
      </fieldset>
      <label>
        Window start
        <input type="datetime-local" value={draft.window_start} onChange={(e) => set({ window_start: e.target.value })} required />
      </label>
      <label>
        Window end
        <input type="datetime-local" value={draft.window_end} onChange={(e) => set({ window_end: e.target.value })} required />
      </label>
      <label>
        Repeats (RRULE, optional)
        <input value={draft.rrule ?? ""} onChange={(e) => set({ rrule: e.target.value })} placeholder="FREQ=WEEKLY;BYDAY=TU" />
      </label>
      <label>
        Note
        <input value={draft.note ?? ""} onChange={(e) => set({ note: e.target.value })} />
      </label>
      <button type="submit">Register expected visitor</button>
    </form>
  );
}
