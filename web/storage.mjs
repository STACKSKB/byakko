// SPDX-License-Identifier: GPL-3.0-or-later
// A completed IndexedDB transaction is the backup-before-write boundary.
export class BackupStore {
  #database;
  constructor(indexedDB = globalThis.indexedDB) {
    this.#database = new Promise((resolve, reject) => {
      const request = indexedDB.open("byakko", 1);
      request.onupgradeneeded = () => {
        request.result.createObjectStore("backups", { keyPath: "id", autoIncrement: true });
      };
      request.onsuccess = () => resolve(request.result);
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
}
