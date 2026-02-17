# Capabilities

This project can do the following (must add tests though):
1. If no matches, rewire the initial composition (pure round trip with no injections)
2. Inject 1 → N middleware(s) _between_ two matched services (they must directly chain together)
3. Inject 1 → N middleware(s) _before_ any service (will place middleware before that service wherever it shows up in the composition)

TODO:
2. Match on a subset of the chain
   - BEFORE: srv → srv-b → srv-c, go before c
   - BETWEEN: srv → srv-b → srv-c, splice between b and c
3. What if there are multiple chains? As in the instance is created with multiple imports
