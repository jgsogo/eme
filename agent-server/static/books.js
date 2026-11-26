(() => {
  const $ = id => document.getElementById(id);
  const state = { books: [], selected: null, collectionQuery: "", sortKey: "title", sortDirection: "asc" };
  async function api(path, options = {}) {
    const response = await fetch(path, { credentials: "same-origin", ...options, headers: { "Content-Type": "application/json", ...(options.headers || {}) } });
    if (!response.ok) throw new Error(await response.text() || `HTTP ${response.status}`);
    return response.status === 204 ? null : response.json();
  }
  if (document.body.dataset.layout === "mobile") {
    initMobileBooks($, api);
    return;
  }

  function initMobileBooks($, api) {
    const state = { books: [], selected: null, query: "", readingStatus: "", wantToBuy: false, selectionRequest: 0, holdingsRequest: 0, catalogueSearchRequest: 0 };
    const status = message => { $("books-status").textContent = message || ""; };
    const readingLabel = value => ({ pending: "To read", reading: "Currently reading", read: "Read" }[value] || "To read");
    const make = (tag, className, value) => {
      const node = document.createElement(tag);
      if (className) node.className = className;
      if (value !== undefined && value !== null) node.textContent = String(value);
      return node;
    };
    const setView = view => {
      document.body.dataset.bookView = view;
      $("books-overview").hidden = view !== "list";
      $("book-detail-view").hidden = view !== "detail";
      $("back-to-books").hidden = view !== "detail";
      $("books-main-heading-copy").hidden = view === "detail";
      $("open-add-book").hidden = view !== "list";
      $("books-main-title").textContent = view === "detail" ? "Book details" : "My collection";
      $("books-main-subtitle").textContent = view === "detail"
        ? "Your reading notes, bibliographic information and library availability"
        : "Your personal book collection and reading list";
    };

    function filteredBooks() {
      const query = state.query.trim().toLocaleLowerCase();
      return state.books.filter(book => {
        const matchesQuery = !query || [
          book.title, ...(book.authors || []), book.isbn, book.publisher,
          book.publication_year, book.reading_status, book.review,
          ...(book.subjects || []), ...(book.notes || []),
          book.want_to_buy ? "want to buy" : ""
        ].join(" ").toLocaleLowerCase().includes(query);
        return matchesQuery
          && (!state.readingStatus || book.reading_status === state.readingStatus)
          && (!state.wantToBuy || book.want_to_buy);
      }).sort((a, b) => new Date(b.updated_at || b.created_at || 0) - new Date(a.updated_at || a.created_at || 0));
    }

    function renderBooks() {
      const list = $("books-list");
      list.replaceChildren();
      const books = filteredBooks();
      $("collection-status").textContent = books.length + " of " + state.books.length + " books";
      if (!books.length) {
        const empty = make("div", "tasks-table-empty");
        const heading = make("strong", "", state.books.length ? "No books match these filters" : "Your collection is empty");
        const description = make("span", "", state.books.length
          ? "Try a different search or clear the reading-status filters."
          : "Search the RABEL catalogue to add your first book.");
        empty.append(heading, description);
        if (!state.books.length) {
          const add = make("button", "secondary", "Find books");
          add.type = "button";
          add.addEventListener("click", () => $("add-book-dialog").showModal());
          empty.append(add);
        } else {
          const clear = make("button", "secondary", "Clear filters");
          clear.type = "button";
          clear.addEventListener("click", () => {
            state.query = "";
            state.readingStatus = "";
            state.wantToBuy = false;
            $("collection-search").value = "";
            $("reading-status-filter").value = "";
            $("want-to-buy-filter").checked = false;
            renderBooks();
          });
          empty.append(clear);
        }
        list.append(empty);
        return;
      }

      for (const book of books) {
        const card = make("button", "item-card book-item-card");
        card.type = "button";
        card.dataset.bookId = book.id;
        card.setAttribute("aria-label", "Open details for " + book.title);
        if (state.selected?.id === book.id) card.setAttribute("aria-current", "true");

        const heading = make("div", "item-card-heading");
        heading.append(make("strong", "", book.title || "Untitled book"));
        if (book.publication_year) heading.append(make("span", "book-card-year", book.publication_year));
        const author = make("p", "item-card-description", (book.authors || []).join(", ") || "Author not specified");
        const meta = make("div", "item-card-meta");
        const statusTag = make("span", "item-card-tag book-status-tag " + (book.reading_status || "pending"), readingLabel(book.reading_status));
        meta.append(statusTag);
        if (book.rating != null) meta.append(make("span", "item-card-tag", "★ " + book.rating + "/5"));
        if (book.want_to_buy) meta.append(make("span", "item-card-tag book-want-tag", "Want to buy"));
        const footnoteParts = [book.publisher, book.publication_year, book.isbn ? "ISBN " + book.isbn : ""].filter(Boolean);
        const footnote = make("div", "item-card-footnote", footnoteParts.join(" · "));
        card.append(heading, author, meta);
        if (footnote.textContent) card.append(footnote);
        card.append(make("span", "item-card-arrow", "›"));
        card.addEventListener("click", () => selectBook(book.id));
        list.append(card);
      }
    }

    function enableDetails(enabled) {
      ["book-title", "book-authors", "book-isbn", "book-rating", "book-status", "book-want-to-buy",
        "book-review", "save-book", "delete-book", "refresh-catalogue", "refresh-holdings"]
        .forEach(id => { $(id).disabled = !enabled; });
    }

    function populateBook(book) {
      $("book-heading").textContent = book.title || "Untitled book";
      $("book-summary").textContent = [
        (book.authors || []).join(", "),
        [book.publisher, book.publication_year].filter(Boolean).join(" · ")
      ].filter(Boolean).join(" — ");
      $("book-title").value = book.title || "";
      $("book-authors").value = (book.authors || []).join(", ");
      $("book-isbn").value = book.isbn || "";
      $("book-rating").value = book.rating ?? "";
      $("book-status").value = book.reading_status || "pending";
      $("book-want-to-buy").checked = Boolean(book.want_to_buy);
      $("book-review").value = book.review || "";

      const badges = $("book-badges");
      badges.replaceChildren();
      badges.append(make("span", "item-card-tag book-status-tag " + (book.reading_status || "pending"), readingLabel(book.reading_status)));
      if (book.rating != null) badges.append(make("span", "item-card-tag", "★ " + book.rating + " / 5"));
      if (book.want_to_buy) badges.append(make("span", "item-card-tag book-want-tag", "Want to buy"));
      renderBookMetadata(book);
    }

    function renderBookMetadata(book) {
      const target = $("book-metadata");
      target.replaceChildren();
      const fields = [
        ["Publisher", book.publisher],
        ["Publication", book.publication_statement],
        ["Year", book.publication_year],
        ["Physical description", book.physical_description],
        ["Series", book.series],
        ["Statement of responsibility", book.responsibility],
        ["Notes", (book.notes || []).join("; ")],
        ["Subjects", (book.subjects || []).join("; ")]
      ].filter(([, value]) => value !== null && value !== undefined && value !== "");
      if (!fields.length) {
        target.append(make("p", "hint", "No additional catalogue details are available yet. Use Refresh to try retrieving the full RABEL record."));
        return;
      }
      for (const [label, value] of fields) {
        const row = make("div", "book-metadata-row");
        row.append(make("dt", "", label), make("dd", "", String(value)));
        target.append(row);
      }
    }

    async function selectBook(id) {
      const request = ++state.selectionRequest;
      state.selected = state.books.find(book => book.id === id) || { id };
      setView("detail");
      renderBooks();
      enableDetails(false);
      $("book-heading").textContent = "Loading book…";
      $("book-summary").textContent = "";
      $("book-badges").replaceChildren();
      $("book-metadata").replaceChildren(make("p", "hint", "Loading bibliographic information…"));
      $("book-holdings").replaceChildren(make("p", "hint", "Loading library availability…"));
      status("Loading book details…");
      try {
        const book = await api("/books/" + encodeURIComponent(id));
        if (request !== state.selectionRequest) return;
        state.selected = book;
        populateBook(book);
        enableDetails(true);
        renderBooks();
        status("");
        await loadHoldings(id);
      } catch (error) {
        if (request !== state.selectionRequest) return;
        status("Could not load this book: " + error.message);
        $("book-holdings").replaceChildren(make("p", "hint", "Availability could not be loaded."));
      }
    }

    async function loadBooks(selectedId = state.selected?.id || null) {
      const books = await api("/books");
      state.books = Array.isArray(books) ? books : [];
      renderBooks();
      if (selectedId && state.books.some(book => book.id === selectedId)) {
        await selectBook(selectedId);
      } else {
        state.selected = null;
        state.selectionRequest++;
        state.holdingsRequest++;
        enableDetails(false);
        setView("list");
      }
    }

    function renderCatalogueItem(result) {
      $("catalogue-item-heading").textContent = result.title || "Catalogue record";
      $("catalogue-item-category").textContent = result.category || "Document type not specified";
      const content = $("catalogue-item-content");
      content.replaceChildren();
      const fields = [
        ["Authors", (result.authors || []).join(", ")],
        ["Statement of responsibility", result.responsibility],
        ["Publisher", result.publisher],
        ["Publication", result.publication_statement || result.publication],
        ["Year", result.publication_year],
        ["Physical description", result.physical_description],
        ["Series", result.series],
        ["Notes", (result.notes || []).join("; ")],
        ["Subjects", (result.subjects || []).join("; ")],
        ["ISBN", result.isbn]
      ].filter(([, value]) => value !== null && value !== undefined && value !== "");
      if (!fields.length) content.append(make("p", "hint", "The search result does not include additional bibliographic details."));
      else {
        const dl = make("dl", "book-metadata");
        for (const [label, value] of fields) {
          const row = make("div", "book-metadata-row");
          row.append(make("dt", "", label), make("dd", "", String(value)));
          dl.append(row);
        }
        content.append(dl);
      }
      $("catalogue-item-dialog").showModal();
    }

    function filterCatalogueResults() {
      const selected = $("catalogue-category-filter").value;
      const cards = [...$("catalogue-results").children];
      let visible = 0;
      for (const card of cards) {
        const matches = !selected || card.dataset.category === selected;
        card.hidden = !matches;
        if (matches) visible++;
      }
      $("catalogue-results-summary").textContent = cards.length
        ? visible + " of " + cards.length + " catalogue results"
        : "No matching records";
    }

    async function addCatalogueResult(result, button) {
      if (button) button.disabled = true;
      try {
        const book = await api("/books", {
          method: "POST",
          body: JSON.stringify({
            title: result.title || "Untitled book",
            authors: result.authors || [],
            isbn: result.isbn || null,
            publisher: result.publisher || null,
            publication_year: result.publication_year || null,
            responsibility: result.responsibility || null,
            publication_statement: result.publication_statement || result.publication || null,
            physical_description: result.physical_description || null,
            series: result.series || null,
            notes: result.notes || [],
            subjects: result.subjects || [],
            catalogue_url: null
          })
        });
        $("add-book-dialog").close();
        $("catalogue-results").replaceChildren();
        $("catalogue-results-summary").textContent = "Enter a search term to find books.";
        $("catalogue-query").value = "";
        $("catalogue-category-filter").replaceChildren(new Option("All categories", ""));
        $("catalogue-filter-controls").hidden = true;
        state.query = "";
        $("collection-search").value = "";
        $("reading-status-filter").value = "";
        $("want-to-buy-filter").checked = false;
        state.readingStatus = "";
        state.wantToBuy = false;
        await loadBooks(book.id);
        status("Book added to your collection.");
      } catch (error) {
        status("Could not add this book: " + error.message);
        if (button) button.disabled = false;
      }
    }

    async function searchCatalogue(event) {
      event.preventDefault();
      const query = $("catalogue-query").value.trim();
      if (!query) return;
      const summary = $("catalogue-results-summary");
      const container = $("catalogue-results");
      const categoryFilter = $("catalogue-category-filter");
      summary.textContent = "Searching RABEL…";
      container.replaceChildren();
      categoryFilter.replaceChildren(new Option("All categories", ""));
      $("catalogue-filter-controls").hidden = true;
      try {
        const request = ++state.catalogueSearchRequest;
        const results = await api("/books/catalogue/search?q=" + encodeURIComponent(query));
        if (request !== state.catalogueSearchRequest) return;
        const categories = [...new Set(results.map(result => result.category || "Document type not specified"))]
          .sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
        for (const category of categories) categoryFilter.add(new Option(category, category));
        $("catalogue-filter-controls").hidden = results.length === 0;
        for (const result of results) {
          const category = result.category || "Document type not specified";
          const card = make("article", "catalogue-entry");
          card.dataset.category = category;
          const details = make("div", "catalogue-entry-details");
          details.append(make("span", "catalogue-entry-category", category));
          const title = make("button", "catalogue-result-title", result.title || "Untitled record");
          title.type = "button";
          title.addEventListener("click", () => renderCatalogueItem(result));
          details.append(title);
          details.append(make("p", "catalogue-entry-author", (result.authors || []).filter(Boolean).join(", ") || "Author not specified"));
          if (result.publisher || result.publication_year || result.isbn) {
            details.append(make("p", "catalogue-entry-meta", [result.publisher, result.publication_year, result.isbn ? "ISBN " + result.isbn : ""].filter(Boolean).join(" · ")));
          }
          const add = make("button", "catalogue-entry-add", "Add to my collection");
          add.type = "button";
          add.dataset.addResult = result.record_ref || result.title || "";
          add.addEventListener("click", () => addCatalogueResult(result, add));
          card.append(details, add);
          container.append(card);
        }
        filterCatalogueResults();
      } catch (error) {
        summary.textContent = "Catalogue search failed";
        container.append(make("p", "hint", error.message));
      }
    }

    async function saveBook(event) {
      event.preventDefault();
      if (!state.selected) return;
      const id = state.selected.id;
      const payload = {
        title: $("book-title").value.trim(),
        authors: $("book-authors").value.split(",").map(value => value.trim()).filter(Boolean),
        isbn: $("book-isbn").value.trim() || null,
        rating: $("book-rating").value ? Number($("book-rating").value) : null,
        reading_status: $("book-status").value,
        want_to_buy: $("book-want-to-buy").checked,
        review: $("book-review").value || null
      };
      const button = $("save-book");
      button.disabled = true;
      try {
        await api("/books/" + encodeURIComponent(id), { method: "PUT", body: JSON.stringify(payload) });
        await loadBooks(state.selected?.id === id ? id : state.selected?.id || null);
        status("Book saved.");
      } catch (error) {
        status("Could not save this book: " + error.message);
      } finally {
        button.disabled = false;
      }
    }

    async function removeBook() {
      if (!state.selected || !confirm("Remove this book from your collection?")) return;
      const id = state.selected.id;
      const button = $("delete-book");
      button.disabled = true;
      try {
        await api("/books/" + encodeURIComponent(id), { method: "DELETE" });
        if (state.selected?.id === id) state.selected = null;
        await loadBooks(state.selected?.id || null);
        status("Book removed from your collection.");
      } catch (error) {
        status("Could not remove this book: " + error.message);
      } finally {
        button.disabled = false;
      }
    }

    async function refreshCatalogue() {
      if (!state.selected) return;
      const id = state.selected.id;
      const button = $("refresh-catalogue");
      button.disabled = true;
      button.textContent = "Refreshing…";
      status("Fetching the latest bibliographic record from RABEL…");
      try {
        const result = await api("/books/" + encodeURIComponent(id) + "/refresh-catalogue", { method: "POST" });
        state.books = state.books.map(book => book.id === id ? result.book : book);
        if (state.selected?.id === id) {
          state.selected = result.book;
          populateBook(result.book);
        }
        renderBooks();
        status("Catalogue information refreshed from RABEL.");
      } catch (error) {
        status("Could not refresh catalogue information: " + error.message);
      } finally {
        button.disabled = false;
        button.textContent = "Refresh";
      }
    }

    async function loadHoldings(id = state.selected?.id) {
      if (!id) return;
      const request = ++state.holdingsRequest;
      const container = $("book-holdings");
      container.replaceChildren(make("p", "hint", "Checking your preferred libraries…"));
      try {
        const data = await api("/books/" + encodeURIComponent(id) + "/holdings");
        if (request !== state.holdingsRequest || state.selected?.id !== id) return;
        container.replaceChildren();
        const libraries = data.libraries || [];
        if (!libraries.length) {
          container.append(make("p", "hint", "Choose preferred libraries to check local availability."));
          return;
        }

        const normalize = value => String(value ?? "").trim().toLocaleLowerCase();
        const recordKey = record => JSON.stringify([
          normalize(record.catalogue_title), (record.authors || []).map(normalize).sort(),
          normalize(record.isbn), normalize(record.publisher), record.publication_year ?? null,
          normalize(record.physical_description), normalize(record.series)
        ]);
        const copyKey = copy => JSON.stringify([
          normalize(copy.signature), normalize(copy.location), normalize(copy.material),
          normalize(copy.availability), normalize(copy.catalogue_note)
        ]);
        const identicalScopes = new Map();
        for (const library of libraries) {
          const records = (library.records || []).map(record => ({
            key: recordKey(record),
            copies: (record.copies || []).map(copyKey).sort(),
            status: record.status || "",
            error: record.error || ""
          })).sort((a, b) => a.key.localeCompare(b.key));
          const fingerprint = JSON.stringify([library.status, library.error || "", records]);
          if (!identicalScopes.has(fingerprint)) identicalScopes.set(fingerprint, { ...library, library_names: [library.library_name] });
          else if (!identicalScopes.get(fingerprint).library_names.includes(library.library_name)) identicalScopes.get(fingerprint).library_names.push(library.library_name);
        }

        const scopes = [...identicalScopes.values()];
        container.append(make("p", "hint book-holdings-summary",
          scopes.length < libraries.length
            ? libraries.length + " selected library entries share " + scopes.length + " distinct catalogue result sets."
            : scopes.length + " distinct catalogue result sets."));
        const recordsByKey = new Map();
        const messages = [];
        for (const library of scopes) {
          if (library.status === "not_found") {
            messages.push({ names: library.library_names, message: "No matching catalogue record" });
            continue;
          }
          if (library.status === "error") {
            messages.push({ names: library.library_names, message: library.error || "Could not query this library" });
            continue;
          }
          for (const record of library.records || []) {
            const key = recordKey(record);
            if (!recordsByKey.has(key)) recordsByKey.set(key, { record, copies: new Map(), libraries: new Set() });
            const grouped = recordsByKey.get(key);
            for (const name of library.library_names) grouped.libraries.add(name);
            for (const copy of record.copies || []) {
              const copyId = copyKey(copy);
              if (!grouped.copies.has(copyId)) grouped.copies.set(copyId, { copy, count: 0, libraries: new Set() });
              const groupedCopy = grouped.copies.get(copyId);
              groupedCopy.count += 1;
              for (const name of library.library_names) groupedCopy.libraries.add(name);
            }
          }
        }

        for (const message of messages) {
          const notice = make("div", "book-holdings-notice");
          notice.append(make("strong", "", message.names.join(" · ")), make("p", "", message.message));
          container.append(notice);
        }

        for (const { record, copies, libraries: names } of recordsByKey.values()) {
          const section = make("section", "book-holding-record");
          section.append(make("h5", "", record.catalogue_title || "Catalogue record"));
          const metadata = [
            (record.authors || []).join(", "),
            [record.publisher, record.publication_year].filter(Boolean).join(", "),
            record.physical_description,
            record.isbn ? "ISBN " + record.isbn : ""
          ].filter(Boolean);
          if (metadata.length) section.append(make("p", "hint", metadata.join(" · ")));
          section.append(make("p", "book-holding-libraries", [...names].join(" · ")));
          if (!copies.size) {
            section.append(make("p", "hint", record.error || "No copy details reported."));
          } else {
            const copyList = make("div", "book-copy-list");
            for (const { copy, count, libraries: copyLibraries } of copies.values()) {
              const item = make("article", "book-copy-card");
              item.append(make("strong", "book-copy-signature", copy.signature || "Library copy"));
              const fields = [
                ["Location", copy.location],
                ["Material", copy.material],
                ["Availability", copy.availability],
                ["Copies", count > 1 ? "×" + count : "1"],
                ["Details", [copy.catalogue_note, copyLibraries.size > 1 ? [...copyLibraries].join(" · ") : ""].filter(Boolean).join(" · ")]
              ].filter(([, value]) => value !== null && value !== undefined && value !== "");
              const dl = make("dl", "book-copy-metadata");
              for (const [label, value] of fields) {
                const row = make("div", "book-copy-field");
                row.append(make("dt", "", label), make("dd", "", String(value)));
                dl.append(row);
              }
              item.append(dl);
              copyList.append(item);
            }
            section.append(copyList);
          }
          container.append(section);
        }

        const checked = scopes.flatMap(library => (library.records || []).map(record => record.checked_at).filter(Boolean));
        if (checked.length) container.append(make("p", "hint book-holdings-checked",
          "Availability checked: " + [...new Set(checked.map(value => new Date(value).toLocaleString()))].join(" · ")));
      } catch (error) {
        if (request === state.holdingsRequest && state.selected?.id === id) {
          container.replaceChildren(make("p", "hint error", "Could not check availability: " + error.message));
        }
      }
    }

    $("collection-search").addEventListener("input", event => { state.query = event.target.value; renderBooks(); });
    $("reading-status-filter").addEventListener("change", event => { state.readingStatus = event.target.value; renderBooks(); });
    $("want-to-buy-filter").addEventListener("change", event => { state.wantToBuy = event.target.checked; renderBooks(); });
    $("back-to-books").addEventListener("click", () => setView("list"));
    $("open-add-book").addEventListener("click", () => $("add-book-dialog").showModal());
    $("close-add-book").addEventListener("click", () => $("add-book-dialog").close());
    $("add-book-dialog").addEventListener("click", event => { if (event.target === $("add-book-dialog")) $("add-book-dialog").close(); });
    $("catalogue-search").addEventListener("submit", searchCatalogue);
    $("catalogue-category-filter").addEventListener("change", filterCatalogueResults);
    $("close-catalogue-item").addEventListener("click", () => $("catalogue-item-dialog").close());
    $("catalogue-item-dialog").addEventListener("click", event => { if (event.target === $("catalogue-item-dialog")) $("catalogue-item-dialog").close(); });
    $("book-form").addEventListener("submit", saveBook);
    $("delete-book").addEventListener("click", removeBook);
    $("refresh-catalogue").addEventListener("click", refreshCatalogue);
    $("refresh-holdings").addEventListener("click", () => loadHoldings());
    enableDetails(false);
    setView("list");
    loadBooks(null).catch(error => {
      $("collection-status").textContent = "Could not load your collection.";
      $("books-list").replaceChildren(make("p", "hint error", error.message));
      status(error.message);
    });
  }

  const status = message => { $("books-status").textContent = message; };
  function button(label, action) { const b = document.createElement("button"); b.type = "button"; b.textContent = label; b.addEventListener("click", action); return b; }
  async function setBookRating(book, rating) {
    try {
      const updated = await api(`/books/${book.id}`, {
        method: "PUT",
        body: JSON.stringify({ rating })
      });
      state.books = state.books.map(item => item.id === updated.id ? updated : item);
      if (state.selected?.id === updated.id) {
        state.selected = updated;
        populateBookForm(updated);
      }
      renderBooks();
      status(`Rating saved for “${updated.title}”.`);
    } catch (error) {
      status(error.message || "Could not save the book rating.");
    }
  }

  function renderBooks() {
    const list = $("books-list"); list.replaceChildren();
    const query = state.collectionQuery.trim().toLocaleLowerCase();
    const filtered = state.books.filter(book => !query || [book.title, ...(book.authors || []), book.isbn || "", book.reading_status || "", book.review || "", book.want_to_buy ? "want to buy" : "", book.rating ?? ""].join(" ").toLocaleLowerCase().includes(query));
    const valueFor = (book, key) => key === "authors" ? (book.authors || []).join(", ").toLocaleLowerCase() : key === "rating" ? (book.rating ?? 0) : key === "want_to_buy" ? (book.want_to_buy ? 1 : 0) : String(book[key] ?? "").toLocaleLowerCase();
    filtered.sort((left, right) => { const x=valueFor(left,state.sortKey), y=valueFor(right,state.sortKey); const result=typeof x==="number"&&typeof y==="number" ? x-y : String(x).localeCompare(String(y),undefined,{numeric:true,sensitivity:"base"}); return result*(state.sortDirection==="asc"?1:-1); });
    $("collection-status").textContent = filtered.length + " of " + state.books.length + " books";
    if (!filtered.length) { const empty=document.createElement("p"); empty.className="books-table-empty"; empty.textContent=state.books.length?"No books match your filter.":"Your collection is empty. Use Add book to find a title in RABEL."; list.append(empty); return; }
    const wrap=document.createElement("div"); wrap.className="books-table-wrap"; const table=document.createElement("table"); table.className="books-table";
    const columns=[["title","Title"],["authors","Author"],["isbn","ISBN"],["reading_status","Reading status"],["rating","Rating"],["want_to_buy","Want to buy"]];
    const thead=document.createElement("thead"), header=document.createElement("tr");
    for (const [key,label] of columns) { const th=document.createElement("th"); const suffix=state.sortKey===key?(state.sortDirection==="asc"?" ↑":" ↓"):""; const sort=button(label+suffix,()=>{if(state.sortKey===key)state.sortDirection=state.sortDirection==="asc"?"desc":"asc";else{state.sortKey=key;state.sortDirection="asc";}renderBooks();}); th.append(sort);header.append(th); }
    thead.append(header);table.append(thead);const tbody=document.createElement("tbody");
    for (const book of filtered) { const row=document.createElement("tr");row.tabIndex=0;row.setAttribute("aria-label","Open details for "+book.title);
      const cells=[[book.title,book.publication_year?String(book.publication_year):""],[(book.authors||[]).join(", ")||"—"],[book.isbn||"—"],[(book.reading_status||"pending").replaceAll("_"," ")],[book.rating==null?"—":book.rating+"/5"],[book.want_to_buy?"Yes":"No"]];
      cells.forEach(([value,sub],index)=>{
        const td=document.createElement("td");
        if (index === 4 && book.reading_status === "read") {
          const rating = document.createElement("div");
          rating.className = "book-rating-stars";
          rating.setAttribute("role", "group");
          rating.setAttribute("aria-label", `Rate ${book.title}`);
          for (let score = 1; score <= 5; score++) {
            const star = button(score <= (book.rating || 0) ? "★" : "☆", event => {
              event.stopPropagation();
              setBookRating(book, score);
            });
            star.className = "book-rating-star";
            star.setAttribute("aria-label", `${score} out of 5 stars`);
            star.setAttribute("aria-pressed", String(book.rating === score));
            rating.append(star);
          }
          td.append(rating);
        } else if (index === 4) {
          td.textContent = book.rating == null ? "Mark as read to rate" : `${book.rating}/5`;
          td.className = "book-rating-unavailable";
        } else {
          td.textContent = value || "—";
          if (index === 0) {
            td.className = "book-title-cell";
            if (sub) { const small=document.createElement("small"); small.textContent=sub; td.append(small); }
          }
        }
        row.append(td);
      });
      row.addEventListener("click",()=>selectBook(book));row.addEventListener("keydown",event=>{if(event.key==="Enter"||event.key===" "){event.preventDefault();selectBook(book);}});tbody.append(row);
    } table.append(tbody);wrap.append(table);list.append(wrap);
  }
  function enableDetails(enabled) {
    ["book-title","book-authors","book-isbn","book-rating","book-status","book-want-to-buy","book-review","save-book","delete-book","refresh-catalogue","refresh-holdings"].forEach(id => $(id).disabled = !enabled);
  }
  function populateBookForm(book) {
    $("book-heading").textContent = book.title;
    $("book-title").value = book.title;
    $("book-authors").value = (book.authors || []).join(", ");
    $("book-isbn").value = book.isbn || "";
    $("book-rating").value = book.rating ?? "";
    $("book-status").value = book.reading_status;
    $("book-want-to-buy").checked = book.want_to_buy;
    $("book-review").value = book.review || "";
  }
  async function refreshCatalogue() {
    if (!state.selected) return;
    const button = $("refresh-catalogue");
    const originalLabel = button.textContent;
    button.disabled = true;
    button.textContent = "Refreshing…";
    status("Fetching the latest bibliographic record from RABEL…");
    try {
      const result = await api("/books/" + state.selected.id + "/refresh-catalogue", { method: "POST" });
      state.selected = result.book;
      state.books = state.books.map(book => book.id === result.book.id ? result.book : book);
      renderBooks();
      populateBookForm(result.book);
      status("Catalogue information refreshed from RABEL.");
    } catch (error) {
      status(error.message || "Could not refresh catalogue information.");
    } finally {
      button.disabled = false;
      button.textContent = originalLabel;
    }
  }
  async function selectBook(book) {
    state.selected = book; renderBooks(); enableDetails(true);
    populateBookForm(book);
    if (!$("book-details-dialog").open) $("book-details-dialog").showModal();
    await loadHoldings();
  }
  async function loadBooks() {
    state.books = await api("/books"); $("collection-status").textContent = `${state.books.length} books in your collection`; renderBooks();
    if (state.selected) { const current = state.books.find(b => b.id === state.selected.id); if (current) await selectBook(current); else { state.selected = null; enableDetails(false); $("book-heading").textContent = "Book details"; $("book-holdings").textContent = "Select a book."; } }
  }
  function showCatalogueItem(result) {
    const dialog = $("catalogue-item-dialog");
    $("catalogue-item-heading").textContent = result.title;
    $("catalogue-item-category").textContent = result.category || "Document type not specified";
    const content = $("catalogue-item-content");
    content.replaceChildren();
    const fields = [
      ["Authors", (result.authors || []).join(", ")],
      ["Statement of responsibility", result.responsibility],
      ["Publisher", result.publisher],
      ["Publication statement", result.publication_statement || result.publication],
      ["Publication year", result.publication_year],
      ["Physical description", result.physical_description],
      ["Series", result.series],
      ["Notes", (result.notes || []).join("; ")],
      ["Subjects", (result.subjects || []).join("; ")],
      ["ISBN", result.isbn]
    ].filter(([, value]) => value !== null && value !== undefined && value !== "");
    if (fields.length) {
      const dl = document.createElement("dl");
      for (const [label, value] of fields) {
        const dt = document.createElement("dt"); dt.textContent = label;
        const dd = document.createElement("dd"); dd.textContent = String(value);
        dl.append(dt, dd);
      }
      content.append(dl);
    } else {
      content.textContent = "The search result does not include additional bibliographic details.";
    }
    dialog.showModal();
  }

  function filterCatalogueResults() {
    const selectedCategory = $("catalogue-category-filter").value;
    const cards = [...$("catalogue-results").children];
    let visible = 0;
    for (const card of cards) {
      const matches = !selectedCategory || card.dataset.category === selectedCategory;
      card.hidden = !matches;
      if (matches) visible++;
    }
    const total = cards.length;
    $("catalogue-results-summary").textContent = total
      ? selectedCategory
        ? `${visible} of ${total} catalogue results · ${selectedCategory}`
        : `${total} catalogue result${total === 1 ? "" : "s"}`
      : "No matching records";
  }

  async function searchCatalogue(event) {
    event.preventDefault(); const q = $("catalogue-query").value.trim(); if (!q) return;
    const summary = $("catalogue-results-summary");
    const container = $("catalogue-results");
    const categoryFilter = $("catalogue-category-filter");
    summary.textContent = "Searching RABEL…";
    container.replaceChildren();
    categoryFilter.replaceChildren(new Option("All document categories", ""));
    $("catalogue-filter-controls").hidden = true;
    try {
      const results = await api(`/books/catalogue/search?q=${encodeURIComponent(q)}`);
      container.replaceChildren();
      const categories = [...new Set(results.map(result => result.category || "Document type not specified"))]
        .sort((left, right) => left.localeCompare(right, undefined, { sensitivity: "base" }));
      for (const category of categories) categoryFilter.add(new Option(category, category));
      $("catalogue-filter-controls").hidden = results.length === 0;
      for (const result of results) {
        const category = result.category || "Document type not specified";
        const card = document.createElement("article");
        card.className = "catalogue-entry";
        card.dataset.category = category;
        const details = document.createElement("div");
        details.className = "catalogue-entry-details";
        const categoryLabel = document.createElement("p");
        categoryLabel.className = "catalogue-entry-category";
        categoryLabel.textContent = category;
        details.append(categoryLabel);
        const author = document.createElement("p");
        author.className = "catalogue-entry-author";
        author.textContent = (result.authors || []).filter(Boolean).join(", ") || "Author not specified";
        details.append(author);
        const openDetails = button(result.title || "Untitled record", () => showCatalogueItem(result));
        openDetails.className = "catalogue-result-title";
        details.append(openDetails);
        card.append(details);
        const add = button("Add to my collection", async () => {
          try {
            const book = await api("/books", { method: "POST", body: JSON.stringify({
              title: result.title,
              authors: result.authors || [],
              isbn: result.isbn,
              publisher: result.publisher,
              publication_year: result.publication_year,
              responsibility: result.responsibility,
              publication_statement: result.publication_statement || result.publication,
              physical_description: result.physical_description,
              series: result.series,
              notes: result.notes || [],
              subjects: result.subjects || [],
              catalogue_url: null
            }) });
            $("add-book-dialog").close(); $("catalogue-results").replaceChildren(); $("catalogue-results-summary").textContent = "Enter a search term to find books."; $("catalogue-query").value = "";
            $("catalogue-category-filter").replaceChildren(new Option("All document categories", "")); $("catalogue-filter-controls").hidden = true;
            await loadBooks(); state.collectionQuery = ""; $("collection-search").value = ""; await selectBook(book); status("Book added.");
          } catch (error) { status(error.message); }
        });
        add.classList.add("catalogue-entry-add");
        card.append(add);
        container.append(card);
      }
      filterCatalogueResults();
    } catch (error) { summary.textContent = "Catalogue search failed"; container.textContent = error.message; }
  }
  async function saveBook(event) {
    event.preventDefault(); if (!state.selected) return;
    const payload = { title: $("book-title").value.trim(), authors: $("book-authors").value.split(",").map(s => s.trim()).filter(Boolean), isbn: $("book-isbn").value.trim() || null, rating: $("book-rating").value ? Number($("book-rating").value) : null, reading_status: $("book-status").value, want_to_buy: $("book-want-to-buy").checked, review: $("book-review").value || null };
    try { await api(`/books/${state.selected.id}`, { method: "PUT", body: JSON.stringify(payload) }); await loadBooks(); status("Book saved."); }
    catch (error) { status(error.message); }
  }
  async function removeBook() {
    if (!state.selected || !confirm("Remove this book from your collection?")) return;
    try { await api(`/books/${state.selected.id}`, { method: "DELETE" }); state.selected = null; enableDetails(false); $("book-heading").textContent = "Book details"; $("book-holdings").textContent = "Select a book."; await loadBooks(); status("Book removed."); }
    catch (error) { status(error.message); }
  }
  async function loadHoldings() {
    if (!state.selected) return;
    const container = $("book-holdings");
    container.textContent = "Checking selected libraries…";
    try {
      const data = await api(`/books/${state.selected.id}/holdings`);
      container.replaceChildren();
      if (!data.libraries.length) {
        container.textContent = "Choose preferred libraries to see copy signatures.";
        return;
      }

      const normalize = value => String(value ?? "").trim().toLocaleLowerCase();
      const recordKey = record => JSON.stringify([
        normalize(record.catalogue_title), (record.authors || []).map(normalize).sort(),
        normalize(record.isbn), normalize(record.publisher), record.publication_year ?? null,
        normalize(record.physical_description), normalize(record.series)
      ]);
      const copyKey = copy => JSON.stringify([
        normalize(copy.signature), normalize(copy.location), normalize(copy.material),
        normalize(copy.availability), normalize(copy.catalogue_note)
      ]);

      // Some RABEL selector entries expose the very same catalogue scope under
      // different labels. Collapse only libraries whose complete result sets
      // are identical; retain their names so the provenance is still visible.
      const identicalScopes = new Map();
      for (const library of data.libraries) {
        const records = (library.records || []).map(record => ({
          key: recordKey(record),
          copies: (record.copies || []).map(copyKey).sort(),
          status: record.status || "",
          error: record.error || ""
        })).sort((a, b) => a.key.localeCompare(b.key));
        const fingerprint = JSON.stringify([library.status, library.error || "", records]);
        if (!identicalScopes.has(fingerprint)) {
          identicalScopes.set(fingerprint, { ...library, library_names: [library.library_name] });
        } else {
          const existing = identicalScopes.get(fingerprint);
          if (!existing.library_names.includes(library.library_name)) existing.library_names.push(library.library_name);
        }
      }
      const scopes = [...identicalScopes.values()];
      const summary = document.createElement("p");
      summary.className = "muted";
      summary.textContent = scopes.length < data.libraries.length
        ? `The ${data.libraries.length} selected library entries expose ${scopes.length} distinct catalogue result sets; identical results are grouped below.`
        : `${scopes.length} distinct catalogue result sets.`;
      container.append(summary);

      const recordsByKey = new Map();
      const messages = [];
      for (const library of scopes) {
        if (library.status === "not_found") {
          messages.push({ names: library.library_names, message: "No matching catalogue record" });
          continue;
        }
        if (library.status === "error") {
          messages.push({ names: library.library_names, message: library.error || "Could not query this library" });
          continue;
        }
        for (const record of library.records || []) {
          const key = recordKey(record);
          if (!recordsByKey.has(key)) {
            recordsByKey.set(key, { record, copies: new Map(), libraries: new Set() });
          }
          const grouped = recordsByKey.get(key);
          for (const name of library.library_names) grouped.libraries.add(name);
          for (const copy of record.copies || []) {
            const key = copyKey(copy);
            if (!grouped.copies.has(key)) grouped.copies.set(key, { copy, count: 0, libraries: new Set() });
            const groupedCopy = grouped.copies.get(key);
            groupedCopy.count += 1;
            for (const name of library.library_names) groupedCopy.libraries.add(name);
          }
        }
      }

      for (const message of messages) {
        const p = document.createElement("p");
        p.className = "muted";
        p.textContent = `${message.names.join(" · ")}: ${message.message}`;
        container.append(p);
      }

      for (const { record, copies, libraries } of recordsByKey.values()) {
        const section = document.createElement("section");
        section.className = "holding";
        const heading = document.createElement("strong");
        heading.textContent = record.catalogue_title || "Catalogue record";
        section.append(heading);
        const metadata = [
          (record.authors || []).join(", "),
          [record.publisher, record.publication_year].filter(Boolean).join(", "),
          record.physical_description,
          record.isbn ? `ISBN ${record.isbn}` : ""
        ].filter(Boolean);
        if (metadata.length) {
          const details = document.createElement("p");
          details.className = "muted";
          details.textContent = metadata.join(" · ");
          section.append(details);
        }
        const libraryNames = document.createElement("p");
        libraryNames.className = "muted";
        libraryNames.textContent = [...libraries].join(" · ");
        section.append(libraryNames);

        if (!copies.size) {
          const empty = document.createElement("p");
          empty.className = "muted";
          empty.textContent = record.error || "No copy details reported.";
          section.append(empty);
        } else {
          const wrap = document.createElement("div");
          wrap.className = "books-table-wrap";
          const table = document.createElement("table");
          table.className = "books-table books-holdings-table";
          const thead = document.createElement("thead");
          const header = document.createElement("tr");
          for (const label of ["Signature", "Location", "Material", "Availability", "Copies", "Additional details"]) {
            const th = document.createElement("th");
            th.textContent = label;
            header.append(th);
          }
          thead.append(header);
          table.append(thead);
          const tbody = document.createElement("tbody");
          for (const { copy, count, libraries: copyLibraries } of copies.values()) {
            const row = document.createElement("tr");
            const values = [
              copy.signature,
              copy.location,
              copy.material,
              copy.availability,
              count > 1 ? `×${count}` : "1",
              [copy.catalogue_note, copyLibraries.size > 1 ? [...copyLibraries].join(" · ") : ""].filter(Boolean).join(" · ")
            ];
            for (const value of values) {
              const cell = document.createElement("td");
              cell.textContent = value === null || value === undefined || value === "" ? "—" : String(value);
              row.append(cell);
            }
            tbody.append(row);
          }
          table.append(tbody);
          wrap.append(table);
          section.append(wrap);
        }
        container.append(section);
      }

      const checked = scopes.flatMap(library => (library.records || []).map(record => record.checked_at).filter(Boolean));
      if (checked.length) {
        const note = document.createElement("p");
        note.className = "muted";
        note.textContent = "Availability checked: " + [...new Set(checked.map(value => new Date(value).toLocaleString()))].join(" · ");
        container.append(note);
      }
    } catch (error) {
      container.textContent = error.message;
    }
  }

  $("collection-search").addEventListener("input", event => { state.collectionQuery = event.target.value; renderBooks(); });
  $("close-book-details").addEventListener("click", () => $("book-details-dialog").close());
  $("book-details-dialog").addEventListener("click", event => { if (event.target === $("book-details-dialog")) $("book-details-dialog").close(); });
  $("close-catalogue-item").addEventListener("click", () => $("catalogue-item-dialog").close());
  $("catalogue-item-dialog").addEventListener("click", event => { if (event.target === $("catalogue-item-dialog")) $("catalogue-item-dialog").close(); });
  $("open-add-book").addEventListener("click", () => $("add-book-dialog").showModal());
  $("close-add-book").addEventListener("click", () => $("add-book-dialog").close());
  $("add-book-dialog").addEventListener("click", event => { if (event.target === $("add-book-dialog")) $("add-book-dialog").close(); });
  $("catalogue-search").addEventListener("submit", searchCatalogue);
  $("catalogue-category-filter").addEventListener("change", filterCatalogueResults);
  $("book-form").addEventListener("submit", saveBook);
  $("delete-book").addEventListener("click", removeBook);
  $("refresh-catalogue").addEventListener("click", refreshCatalogue);
  $("refresh-holdings").addEventListener("click", loadHoldings);
  enableDetails(false);
  loadBooks().catch(error => status(error.message));
})();