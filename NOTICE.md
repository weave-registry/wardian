# Notices

## The Elephant-Goldfish Model

Make an app's fresh reviews (`fresh_review`, ADR-2610281000) and the app-factory skill's goldfish
reviews follow the Elephant-Goldfish Model (EGM) by Google's EGM team:

- Website: <https://egm.dev>
- Repository: <https://github.com/google/elephant-goldfish-model>
- Licence: Apache License 2.0
- The idea: Dave Rensin, "Elephants, Goldfish, and the New Golden Age of Software Engineering"
  (<https://medium.com/@drensin/elephants-goldfish-and-the-new-golden-age-of-software-engineering-c33641a48874>)

An agent that holds the whole context (the elephant) drafts; reviewers that hold none of it (the
goldfish) review; a person settles the trade-offs. The review's lenses (scope, design, readiness to
build, comprehension) are modelled on EGM's reviewers. Wardian's prompts are its own and no EGM text
is copied. When the EGM plugin is installed, the app-factory skill calls its `egm-brainstorm` and
`egm-review` skills, which stay EGM's, under its licence.

Thank you to the EGM team.
