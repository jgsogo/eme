CREATE TABLE school_menus (
 id INTEGER PRIMARY KEY AUTOINCREMENT,
 date TEXT NOT NULL UNIQUE CHECK(length(date)=10),
 status TEXT NOT NULL CHECK(status IN ('known','not_published','closed','unknown')),
 payload TEXT NOT NULL CHECK(json_valid(payload)),
 source TEXT, imported_at INTEGER,
 origin TEXT NOT NULL CHECK(origin IN ('manual','import')),
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE INDEX school_menus_date ON school_menus(date);