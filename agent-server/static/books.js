(() => {
  const $ = id => document.getElementById(id);
  const state = { books: [], selected: null, collectionQuery: "" };
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
    const filtered = state.books.filter(book => !query || [book.title, ...(book.authors || []), book.isbn || ""].join(" ").toLocaleLowerCase().includes(query));
    if (!filtered.length) { list.textContent = state.books.length ? "No books match your search." : "Your collection is empty. Use Add to find a book in RABEL."; return; }
    for (const book of filtered) {
      const item = document.createElement("div"); item.className = "book-entry" + (state.selected?.id === book.id ? " active" : "");
      const title = document.createElement("strong"); title.textContent = book.title; item.append(title);
      const meta = document.createElement("small"); meta.textContent = ` · ${(book.authors || []).join(", ")} · ${book.reading_status}`; item.append(meta);
      item.addEventListener("click", () => selectBook(book)); list.append(item);
    }
  }
  function enableDetails(enabled) {
    ["book-title","book-authors","book-isbn","book-rating","book-status","book-want-to-buy","book-review","save-book","delete-book","refresh-holdings"].forEach(id => $(id).disabled = !enabled);
  }
  async function selectBook(book) {
    state.selected = book; renderBooks(); enableDetails(true);
    $("book-heading").textContent = book.title; $("book-title").value = book.title;
    $("book-authors").value = book.authors.join(", "); $("book-isbn").value = book.isbn || "";
    $("book-rating").value = book.rating ?? ""; $("book-status").value = book.reading_status;
    $("book-want-to-buy").checked = book.want_to_buy; $("book-review").value = book.review || "";
    await loadHoldings();
  }
  async function loadBooks() {
    state.books = await api("/books"); $("collection-status").textContent = `${state.books.length} books in your collection`; renderBooks();
    if (state.selected) { const current = state.books.find(b => b.id === state.selected.id); if (current) await selectBook(current); else { state.selected = null; enableDetails(false); $("book-heading").textContent = "Book details"; $("book-holdings").textContent = "Select a book."; } }
  }
  async function searchCatalogue(event) {
    event.preventDefault(); const q = $("catalogue-query").value.trim(); if (!q) return;
    $("catalogue-results").textContent = "Searching RABEL…";
    try {
      const results = await api(`/books/catalogue/search?q=${encodeURIComponent(q)}`);
      const container = $("catalogue-results"); container.replaceChildren();
      for (const result of results) {
        const card = document.createElement("div"); card.className = "catalogue-entry";
        const title = document.createElement("strong"); title.textContent = result.title; card.append(title);
        const meta = document.createElement("p"); meta.textContent = [result.authors.join(", "), result.isbn].filter(Boolean).join(" · "); card.append(meta);
        card.append(button("Add to my collection", async () => {
          try {
            const book = await api("/books", { method: "POST", body: JSON.stringify({ title: result.title, authors: result.authors, isbn: result.isbn, catalogue_url: null }) });
            $("add-book-dialog").close(); $("catalogue-results").replaceChildren(); $("catalogue-query").value = "";
            await loadBooks(); state.collectionQuery = ""; $("collection-search").value = ""; await selectBook(book); status("Book added.");
          } catch (error) { status(error.message); }
        }));
        container.append(card);
      }
      if (!results.length) container.textContent = "No matching catalogue records.";
    } catch (error) { $("catalogue-results").textContent = error.message; }
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
  $("open-add-book").addEventListener("click", () => $("add-book-dialog").showModal());
  $("close-add-book").addEventListener("click", () => $("add-book-dialog").close());
  $("add-book-dialog").addEventListener("click", event => { if (event.target === $("add-book-dialog")) $("add-book-dialog").close(); });
  $("catalogue-search").addEventListener("submit", searchCatalogue);
  $("book-form").addEventListener("submit", saveBook);
  $("delete-book").addEventListener("click", removeBook);
  $("refresh-holdings").addEventListener("click", loadHoldings);
  enableDetails(false);
  loadBooks().catch(error => status(error.message));
})();