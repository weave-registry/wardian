/* The sample meeting behind "Try the sample": notes and a little transcript, with every pattern the
   quick reader knows, so both readers have something to find. */
const SAMPLE_NOTES = Object.freeze({
  title: 'Mobile app launch: weekly sync',
  text: `# Mobile app launch: weekly sync
Attendees: Priya (product), Marco (engineering), Lena (design), Sam (support)

## Status
- Beta has 412 testers; crash-free sessions at 99.2%, up from 97.8% last week.
- The onboarding redesign is done in Figma. Two screens still need copy.
- App Store review for 2.0 was rejected once over the account-deletion flow; fix is in review.

## Discussion
Priya: The launch date depends on the store review. If we are approved by the 20th we keep the 27th.
Marco: The deletion fix is small. I'll resubmit the build tomorrow morning.
Lena: I can finish the onboarding copy if someone from support reviews the wording.
Sam: Happy to. I will review the onboarding copy by Thursday.
Priya: Should we hold the press release until approval, or prepare both versions?

Decision: Launch date stays October 27 if Apple approves by October 20.
Decision: Dark mode moves to 2.1; it will not block the launch.
Agreed to keep the beta open for one more week.

## Next steps
Action: Marco to resubmit the 2.0 build to App Store review, due Friday
TODO: Update the support macros for the new sign-in screen (@sam)
@lena will send final onboarding copy to Sam by Wednesday
- [ ] Priya: draft the launch email and the press release, by Oct 22

## Open
Q: Do we need a separate privacy notice for the EU testers?
Who owns the launch-day status page?
`,
});
