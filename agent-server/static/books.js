(() => {
  const $ = id => document.getElementById(id);
  const state = { books: [], selected: null, collectionQuery: "", sortKey: "title", sortDirection: "asc" };
  async function api(path, options = {}) {
    const response = await fetch(path, { credentials: "same-origin", ...options, headers: { "Content-Type": "application/json", ...(options.headers || {}) } });
    if (!response.ok) throw new Error(await response.text() || `HTTP ${response.status}`);
    return response.status === 204 ? null : response.json();
  }
  const status = message => { $("books-status").textContent = message; };
  function button(label, action) { const b = document.createElement("button"); b.type = "button"; b.textContent = label; b.addEventListener("click", action); return b; }
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
      cells.forEach(([value,sub],index)=>{const td=document.createElement("td");td.textContent=value||"—";if(index===0){td.className="book-title-cell";if(sub){const small=document.createElement("small");small.textContent=sub;td.append(small);}}row.append(td);});
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
    categoryFilter.hidden = true;
    try {
      const results = await api(`/books/catalogue/search?q=${encodeURIComponent(q)}`);
      container.replaceChildren();
      const categories = [...new Set(results.map(result => result.category || "Document type not specified"))]
        .sort((left, right) => left.localeCompare(right, undefined, { sensitivity: "base" }));
      for (const category of categories) categoryFilter.add(new Option(category, category));
      categoryFilter.hidden = results.length === 0;
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
            $("catalogue-category-filter").replaceChildren(new Option("All document categories", "")); $("catalogue-category-filter").hidden = true;
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