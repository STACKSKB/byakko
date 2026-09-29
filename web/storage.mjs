// SPDX-License-Identifier: GPL-3.0-or-later
// A completed IndexedDB transaction is the backup-before-write boundary.
export class BackupStore {
  #database;
  constructor(indexedDB = globalThis.indexedDB) {
    this.#database = new Promise((resolve, reject) => {
      const request = indexedDB.open("byakko", 2);
      request.onupgradeneeded = () => {
        if (!request.result.objectStoreNames.contains("backups")) {
          request.result.createObjectStore("backups", { keyPath: "id", autoIncrement: true });
        }
        if (!request.result.objectStoreNames.contains("macroNames")) {
          request.result.createObjectStore("macroNames", { keyPath: "id" });
        }
      };
      request.onsuccess = () => {
        const database = request.result;
        database.onversionchange = () => database.close();
        resolve(database);
      };
      request.onerror = () => reject(request.error);
      request.onblocked = () => reject(new Error("Close other Byakko tabs to open backup storage."));
    });
    // An unavailable store is reported when a save requests its backup.
    this.#database.catch(() => {});
  }

  async save(record, device) {
    const database = await this.#database;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("backups", "readwrite", { durability: "strict" });
      transaction.objectStore("backups").add({
        createdAt: new Date().toISOString(),
        device: { vendorId: device.vendorId, productId: device.productId, productName: device.productName },
        record,
      });
      transaction.oncomplete = () => resolve();
      transaction.onabort = () => reject(transaction.error ?? new Error("Backup transaction aborted."));
      transaction.onerror = () => reject(transaction.error);
    });
  }

  async all() {
    const database = await this.#database;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("backups", "readonly");
      const request = transaction.objectStore("backups").getAll();
      transaction.oncomplete = () => resolve(request.result);
      transaction.onabort = () => reject(transaction.error);
      transaction.onerror = () => reject(transaction.error);
    });
  }

  async macroNames(device) {
    const database = await this.#database;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("macroNames", "readonly");
      const request = transaction.objectStore("macroNames").getAll();
      transaction.oncomplete = () => {
        const names = Object.fromEntries(request.result
          .filter(record => record.vendorId === device.vendorId && record.productId === device.productId)
          .map(record => [record.slot, record.name]));
        resolve(names);
      };
      transaction.onabort = () => reject(transaction.error ?? new Error("Local macro names could not be read."));
      transaction.onerror = () => reject(transaction.error);
    });
  }

  async saveMacroName(device, slot, name) {
    const database = await this.#database;
    return new Promise((resolve, reject) => {
      const transaction = database.transaction("macroNames", "readwrite", { durability: "strict" });
      const id = `${device.vendorId}:${device.productId}:${slot}`;
      if (name.trim()) transaction.objectStore("macroNames").put({ id, vendorId: device.vendorId, productId: device.productId, slot, name });
      else transaction.objectStore("macroNames").delete(id);
      transaction.oncomplete = () => resolve();
      transaction.onabort = () => reject(transaction.error ?? new Error("Local macro name was not saved."));
      transaction.onerror = () => reject(transaction.error);
    });
  }
}
