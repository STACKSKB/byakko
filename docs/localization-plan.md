# Localization plan

The source-only pre-alpha currently presents English UI text. Localization is
planned; no translated-language support is claimed yet. Required initial
languages are English (`en`), Hindi (`hi`), Bengali (`bn`), Kannada (`kn`),
Telugu (`te`), Tamil (`ta`), Marathi (`mr`) and Japanese (`ja`).

## Boundary

Keep core IDs, enum variants, protocol data, serialized commands, file formats
and state transitions independent of rendered text. Translation must not change
an opcode, a key/feature ID or the interpretation of saved data. Preserve
backend-provided opaque values without using their labels as identifiers.

Catalog work belongs to frontend presentation. Inventory visible static text
and typed error/recovery labels first, then give them stable message keys and
explicit placeholder values. Keep the English wording alongside each key for
review. Preserve raw diagnostic detail as supporting evidence rather than
parsing it to choose a translated outcome.

## Implementation sequence after functional acceptance

1. Inventory shared controls, page labels, dialogs and typed status messages;
   resolve duplicated terminology before translation.
2. Propose the catalog format and language-selection behavior for user approval.
   Decide whether the app follows the OS, remembers a choice, or offers both;
   also approve fallback behavior. Do not add a new settings flow implicitly.
3. Route the English presentation through the catalog without changing state or
   protocol behavior. Check missing message keys and placeholder consistency.
4. Obtain reviewed translations for the required languages. Verify Indic shaping,
   Japanese glyphs, suitable installed fonts, wrapping and long labels in the
   persistent keyboard workspace. Avoid fixed English-width assumptions.
5. Test switching languages and loading files produced in another language;
   transport and file semantics must stay identical. Record actual reviewed
   language coverage in the support matrix before advertising it.

This plan does not authorize translation polish or a new preferences UI ahead
of the remaining device work. Product behavior choices in step 2 remain with
the user; catalog plumbing is not a reason to alter the native architecture.
