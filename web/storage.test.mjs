import test from "node:test";
import assert from "node:assert/strict";
import { BackupStore } from "./storage.mjs";

test("version-two upgrade preserves backups and stores names only for the selected product", async () => {
  const stores = new Set(["backups"]);
  const records = new Map();
  let closed = false;
  const database = {
    close() { closed = true; },
    objectStoreNames: { contains: name => stores.has(name) },
    createObjectStore(name) { stores.add(name); },
    transaction(name) {
      const transaction = {
        objectStore() {
          return {
            getAll() { return { result: [...records.values()] }; },
            put(record) { records.set(record.id, record); },
            delete(id) { records.delete(id); },
          };
        },
      };
      assert.equal(stores.has(name), true);
      queueMicrotask(() => transaction.oncomplete());
      return transaction;
    },
  };
  const indexedDB = {
    open(name, version) {
      assert.equal(name, "byakko");
      assert.equal(version, 2);
      const request = { result: database };
      queueMicrotask(() => { request.onupgradeneeded(); request.onsuccess(); });
      return request;
    },
  };
  const storage = new BackupStore(indexedDB);
  const selected = { vendorId: 0x3151, productId: 0x4015 };
  await storage.saveMacroName(selected, "slot-1", "Launch editor");
  assert.deepEqual(await storage.macroNames(selected), { "slot-1": "Launch editor" });
  assert.deepEqual(await storage.macroNames({ vendorId: 0x3151, productId: 0x4011 }), {});
  await storage.saveMacroName(selected, "slot-1", "");
  assert.deepEqual(await storage.macroNames(selected), {});
  assert.deepEqual([...stores], ["backups", "macroNames"]);
  database.onversionchange();
  assert.equal(closed, true);
});
