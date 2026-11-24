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
  function renderBookBibliography(book) {
    const container = $("book-bibliography");
    container.replaceChildren();
    const fields = [
      ["Publisher", book.publisher],
      ["Publication year", book.publication_year],
      ["Statement of responsibility", book.responsibility],
      ["Publication statement", book.publication_statement],
      ["Physical description", book.physical_description],
      ["Series", book.series],
      ["Notes", (book.notes || []).join("; ")],
      ["Subjects", (book.subjects || []).join("; ")]
    ].filter(([, value]) => value !== null && value !== undefined && value !== "");
    if (!fields.length) {
      container.textContent = "No catalogue metadata loaded. Use Refresh catalogue info to fetch it from RABEL.";
      return;
    }
    const dl = document.createElement("dl");
    for (const [label, value] of fields) {
      const dt = document.createElement("dt"); dt.textContent = label;
      const dd = document.createElement("dd"); dd.textContent = String(value);
      dl.append(dt, dd);
    }
    container.append(dl);
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
    renderBookBibliography(book);
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
  async function showCatalogueItem(result) {
    const dialog = $("catalogue-item-dialog");
    $("catalogue-item-heading").textContent = result.title;
    $("catalogue-item-category").textContent = result.category || "Document type not specified";
    const content = $("catalogue-item-content");
    content.textContent = "Loading bibliographic details…";
    dialog.showModal();
    try {
      const item = await api("/books/catalogue/item?record_ref=" + encodeURIComponent(result.record_ref));
      $("catalogue-item-heading").textContent = item.title || result.title;
      content.replaceChildren();
      const fields = [
        ["Authors", (item.authors || []).join(", ")],
        ["Statement of responsibility", item.responsibility],
        ["Publisher", item.publisher],
        ["Publication statement", item.publication_statement || result.publication],
        ["Publication year", item.publication_year],
        ["Physical description", item.physical_description],
        ["Series", item.series],
        ["Notes", (item.notes || []).join("; ")],
        ["Subjects", (item.subjects || []).join("; ")],
        ["ISBN", item.isbn]
      ].filter(([,value]) => value !== null && value !== undefined && value !== "");
      if (fields.length) {
        const dl = document.createElement("dl");
        for (const [label,value] of fields) {
          const dt=document.createElement("dt"); dt.textContent=label;
          const dd=document.createElement("dd"); dd.textContent=String(value);
          dl.append(dt,dd);
        }
        content.append(dl);
      }

    } catch (error) { content.textContent=error.message || "Could not load catalogue details."; }
  }

  async function searchCatalogue(event) {
    event.preventDefault(); const q = $("catalogue-query").value.trim(); if (!q) return;
    const summary = $("catalogue-results-summary");
    const container = $("catalogue-results");
    summary.textContent = "Searching RABEL…";
    container.replaceChildren();
    try {
      const results = await api(`/books/catalogue/search?q=${encodeURIComponent(q)}`);
      container.replaceChildren();
      summary.textContent = results.length ? `${results.length} catalogue result${results.length === 1 ? "" : "s"}` : "No matching records";
      for (const result of results) {
        const card = document.createElement("article"); card.className = "catalogue-entry";
        const details = document.createElement("div"); details.className = "catalogue-entry-details";
        const title = document.createElement("strong"); title.className = "catalogue-entry-title"; title.textContent = result.title; details.append(title);
        const meta = document.createElement("p"); meta.className = "catalogue-entry-meta"; meta.textContent = result.category || "Document type not specified"; details.append(meta);
        const openDetails = button(result.title, () => showCatalogueItem(result));
        openDetails.className = "catalogue-result-title";
        details.replaceChild(openDetails, title);
        card.append(details);
        const add = button("Add to my collection", async () => {
          try {
            const item = await api("/books/catalogue/item?record_ref=" + encodeURIComponent(result.record_ref));
            const book = await api("/books", { method: "POST", body: JSON.stringify({
              title: item.title || result.title,
              authors: item.authors || result.authors || [],
              isbn: item.isbn || result.isbn,
              publisher: item.publisher,
              publication_year: item.publication_year,
              responsibility: item.responsibility,
              publication_statement: item.publication_statement,
              physical_description: item.physical_description,
              series: item.series,
              notes: item.notes || [],
              subjects: item.subjects || [],
              catalogue_url: null
            }) });
            $("add-book-dialog").close(); $("catalogue-results").replaceChildren(); $("catalogue-results-summary").textContent = "Enter a search term to find books."; $("catalogue-query").value = "";
            await loadBooks(); state.collectionQuery = ""; $("collection-search").value = ""; await selectBook(book); status("Book added.");
          } catch (error) { status(error.message); }
        });
        add.classList.add("catalogue-entry-add");
        card.append(add);
        container.append(card);
      }
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
    const container = $("book-holdings"); container.textContent = "Checking selected libraries…";
    try {
      const data = await api(`/books/${state.selected.id}/holdings`); container.replaceChildren();
      if (!data.libraries.length) { container.textContent = "Choose preferred libraries to see copy signatures."; return; }
      for (const library of data.libraries) {
        const section = document.createElement("section"); section.className = "holding";
        const heading = document.createElement("strong"); heading.textContent = library.library_name; section.append(heading);
        if (library.status === "not_found") section.append(document.createTextNode(" · No matching catalogue record."));
        else if (library.status === "error") section.append(document.createTextNode(` · ${library.error}`));
        else {
          const timestamp = document.createElement("small"); timestamp.textContent = library.checked_at ? ` · Checked ${new Date(library.checked_at).toLocaleString()}` : ""; section.append(timestamp);
          for (const copy of library.copies) { const line = document.createElement("p"); line.textContent = [copy.signature, copy.location, copy.material, copy.availability].filter(Boolean).join(" · ") || copy.catalogue_note || "Copy details unavailable"; section.append(line); }
          if (!library.copies.length) section.append(document.createTextNode(" · No copy details reported."));
        }
        container.append(section);
      }
    } catch (error) { container.textContent = error.message; }
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
  $("book-form").addEventListener("submit", saveBook);
  $("delete-book").addEventListener("click", removeBook);
  $("refresh-catalogue").addEventListener("click", refreshCatalogue);
  $("refresh-holdings").addEventListener("click", loadHoldings);
  enableDetails(false);
  loadBooks().catch(error => status(error.message));
})();