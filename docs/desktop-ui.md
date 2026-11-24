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
