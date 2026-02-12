/**
 * Utility functions for common operations.
 */

function debounce(fn, delay) {
  let timer = null;
  return function (...args) {
    clearTimeout(timer);
    timer = setTimeout(() => fn.apply(this, args), delay);
  };
}

function throttle(fn, limit) {
  let lastCall = 0;
  return function (...args) {
    const now = Date.now();
    if (now - lastCall >= limit) {
      lastCall = now;
      return fn.apply(this, args);
    }
  };
}

class EventEmitter {
  constructor() {
    this.listeners = {};
  }

  on(event, callback) {
    if (!this.listeners[event]) {
      this.listeners[event] = [];
    }
    this.listeners[event].push(callback);
  }

  emit(event, ...args) {
    const handlers = this.listeners[event];
    if (handlers) {
      handlers.forEach((handler) => handler(...args));
    }
  }

  off(event, callback) {
    const handlers = this.listeners[event];
    if (handlers) {
      this.listeners[event] = handlers.filter((h) => h !== callback);
    }
  }
}

function deepClone(obj) {
  if (obj === null || typeof obj !== "object") {
    return obj;
  }
  if (Array.isArray(obj)) {
    return obj.map(deepClone);
  }
  return Object.fromEntries(
    Object.entries(obj).map(([key, val]) => [key, deepClone(val)])
  );
}

const MAX_RETRY_COUNT = 3;
const DEFAULT_DELAY = 1000;

module.exports = { debounce, throttle, EventEmitter, deepClone };
