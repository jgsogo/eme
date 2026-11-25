# Desktop web console UI principles

These principles apply to the desktop web console. They consolidate the existing UI patterns; they are not a request to rebuild every view from scratch.

## Shared application chrome

- Every desktop view must use the same header: same structure, styling, dimensions, navigation and behavior.
- Reuse the existing shared header implementation. Do not create view-specific header variants.
- Keep the main content area at a consistent, centered width across all views. A view with a submenu and a view without one must have the same content width; neither should expand to the full screen.
- The desktop content container currently uses a maximum width of 1440px.

## Section navigation

- Some views have a section submenu; others go directly to their content. Both layouts must align to the same main content width.
- Keep submenus visually clean and lightweight.
- Place the submenu outside rounded content panels/sections. It is navigation, not a content card.
- Reuse the existing submenu/navigation component and consistent active, hover and spacing styles.

## Content layout

- Reuse existing components and established patterns wherever possible. Prefer consolidating shared styles and behavior over introducing parallel implementations.
- A view may use a left pane alongside its main content when that helps navigation or selection. Other views may use the full content area for their main section.
- Keep the hierarchy clear: navigation first, then the content area. Avoid unnecessary nested rounded panels.
- When the main section presents a collection of records, use a table when columns and comparison make the information easier to scan. A simple list is also appropriate when the items are primarily selectable or have a compact, descriptive presentation.
- The choice between a table and a list does not change the filtering and add-action expectations below.

## Lists, tables and actions

- Every list or table must provide a text field to filter its entries.
- Add binary checkbox filters when they are useful for the collection, such as showing only enabled entries or hiding already-purchased items. Do not add meaningless filters just for uniformity.
- Keep the **Add** action close to the list or table it affects, normally in the same heading or toolbar.
- Show the **Add** action only for collections that the user can extend. Do not add it to read-only histories or derived collections.
- Keep filter controls and collection actions together in a compact, consistent toolbar. Filtering should update the displayed entries without changing the underlying data.
- Preserve useful empty states, including a distinct message when filters produce no matches.

## Details and multi-input actions

- Prefer a modal when the current view does not have enough room to show object details or an operation that requires several user inputs.
- Use the modal to keep the user in context rather than forcing a cramped inline form or an unnecessary navigation away from the collection.
- Reuse the existing modal and form components, validation and action patterns.

## Responsive boundary

- These principles describe the desktop web console only.
- The mobile UI is isolated and must remain unchanged by desktop UI consolidation. Keep desktop-only layout and styling changes scoped so they do not alter mobile views, mobile components or mobile stylesheets.

## Consistency without uniformity

- Consistency does not mean uniformity. Use the same interaction patterns and visual language across views, but choose the layout and controls that best fit each view's purpose.
- Do not force every screen into the same template when its content or workflow calls for a different structure.

## Interaction states and feedback

- Make asynchronous work visible with clear loading or in-progress states.
- Distinguish between an empty collection, a loading collection, an error, and a search/filter result with no matches.
- Provide clear success or failure feedback after actions that modify data.
- Prevent accidental duplicate submissions while an operation is in progress.
- Preserve the user's context and entered input when an operation fails.

## Accessibility and keyboard interaction

- All interactive elements must be usable with a keyboard.
- Provide visible focus indicators and accessible labels.
- Use semantic HTML and appropriate accessible roles.
- Dialogs must manage focus correctly, support Escape where appropriate, and return focus to the element that opened them.
- Do not rely on color alone to communicate status, meaning, or validation.

## Collection navigation and presentation

- Provide sorting when the displayed fields have meaningful ordering.
- Keep column headings and values aligned and easy to scan.
- Avoid unnecessary columns; prioritize information needed to identify entries and act on them.
- Consider pagination or progressive loading for large collections.
- Preserve filters and sorting when navigating away and returning, where practical.
- Apply these behaviors when useful; do not add sorting or pagination where they do not help.

## Forms and destructive actions

- Validate inputs and explain errors next to the relevant fields.
- Preserve entered values when validation or server-side operations fail.
- Distinguish primary and secondary actions clearly.
- Require confirmation for destructive or difficult-to-reverse actions.
- Explain the consequences of a destructive action before asking for confirmation.

## Visual consistency

- Reuse consistent spacing, typography, colors, borders, and control sizes.
- Maintain a clear visual hierarchy for page titles, section headings, descriptions, and actions.
- Prefer existing components and visual patterns over introducing new ones for the same purpose.
- Use status indicators consistently across views.

## Implementation and regression boundaries

- Prefer shared components and styles over duplicated view-specific implementations.
- Scope styles to the relevant component or desktop layout. Avoid broad selectors that can cause unintended changes elsewhere.
- Check desktop changes at representative viewport sizes, including around the desktop/mobile breakpoint.
- Verify that desktop changes do not regress existing mobile behavior.
