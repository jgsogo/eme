(() => {
  const STORAGE_KEY = "eme.meals.selected-family";
  let families = [];
  let selectedFamilyId = localStorage.getItem(STORAGE_KEY) || "";

  function currentFamilyId() { return selectedFamilyId; }
  function render() {
    const mount = document.getElementById("meals-family-selector");
    if (!mount) return;
    mount.innerHTML = '<label for="meals-family-select">Family</label><select id="meals-family-select" aria-label="Selected family"></select>';
    const select = mount.querySelector("select");
    select.innerHTML = families.map(f => '<option value="' + String(f.id).replace(/&/g, "&amp;").replace(/"/g, "&quot;") + '">' +
      String(f.name).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;") + '</option>').join("");
    if (!families.length) {
      mount.innerHTML = '<p class="hint">No family available. <a href="/console/families">Create or join a family</a> to use Meals.</p>';
      selectedFamilyId = "";
      localStorage.removeItem(STORAGE_KEY);
      return;
    } else {
      if (!families.some(f => String(f.id) === selectedFamilyId)) selectedFamilyId = String(families[0].id);
      select.value = selectedFamilyId;
      select.addEventListener("change", () => {
        selectedFamilyId = select.value;
        localStorage.setItem(STORAGE_KEY, selectedFamilyId);
        window.dispatchEvent(new CustomEvent("eme:family-changed", { detail: { familyId: selectedFamilyId } }));
      });
    }
    localStorage.setItem(STORAGE_KEY, selectedFamilyId);
  }

  async function init() {
    const response = await fetch("/families");
    if (!response.ok) throw new Error("Unable to load families");
    const payload = await response.json();
    if (!Array.isArray(payload)) throw new Error("The families endpoint returned an invalid response");
    families = payload.filter(family => family && typeof family.id === "string" && family.id.length > 0);
    render();
    return { families, familyId: selectedFamilyId };
  }

  window.emeMealsFamily = { init, currentFamilyId, families: () => families };
})();