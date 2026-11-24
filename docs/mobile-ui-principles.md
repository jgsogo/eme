# Mobile UI principles

This document records the design constraints for the mobile console. It describes the intended mobile experience and the boundaries that future changes should preserve.

## Scope and separation

- Treat the mobile console as its own presentation, not as a responsive rearrangement of the desktop console.
- Keep mobile-specific markup, styles, and behavior isolated. Do not change desktop UI as a side effect of mobile work.
- Share implementation only when it is genuinely useful and does not couple the two presentation modes. That is a future option, not a current requirement.
- Apply mobile-only behavior only to pages explicitly marked with `data-layout="mobile"`.

## Navigation

- Use a persistent bottom navigation bar for the five primary destinations: **Chats**, **Notes**, **Tasks**, **Meals**, and **Profile**.
- Keep the primary navigation compact and icon-only to preserve scarce screen space.
- Icons must remain understandable and accessible: every destination needs an accessible name (for example, `aria-label`) and a useful tooltip/title. Decorative SVGs should be hidden from assistive technology.
- Expose the current destination visually and semantically (including `aria-current="page"`).
- Keep secondary navigation within its owning section. For example, meal-planning, shopping-list, and school-menu pages belong under Meals; families and integrations belong under Profile. They should highlight their parent destination rather than adding more primary tabs.
- Keep the navigation destinations and their active-route matching together so route changes are easy to maintain.

## Mobile interaction

- Prefer compact controls and clear, familiar icons where that improves information density without sacrificing accessibility.
- Preserve useful mobile-specific interactions, such as the conversations drawer, rather than replacing them as part of a navigation-only change.
- A drawer must have an obvious open/close path, support backdrop dismissal and Escape, and communicate its expanded state to assistive technology.
- Do not offer a desktop/mobile view toggle inside the mobile UI. The mobile pages are already the mobile experience; a mode switch consumes space and adds a choice users do not need there.

## Layout and safe areas

- Keep the header compact; the bottom navigation is the persistent primary navigation.
- Reserve enough space below page content for the fixed bottom bar so controls and content are not obscured.
- Respect device safe areas, especially the bottom inset on devices with gesture indicators or a home indicator.
- Ensure the navigation remains usable on narrow screens and does not depend on text labels being visible.

## Change checklist

When changing the mobile console, verify that:

1. All mobile pages use the same five primary destinations.
2. The correct primary destination is active on both top-level and nested pages.
3. Each icon-only link has an accessible name and visible focus behavior.
4. The fixed navigation does not cover scrollable content or page controls.
5. The conversations drawer still opens and closes through its supported interactions.
6. Desktop pages and desktop navigation remain unchanged.

These principles are intentionally small and practical. They should guide incremental improvements without turning the mobile console into a separate framework or a larger redesign.
