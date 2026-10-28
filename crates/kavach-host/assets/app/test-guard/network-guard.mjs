// Network guard for generated tests.
//
// Node 20's permission model covers the filesystem and child processes, but not the network.
// A generated test could therefore open a socket, which contradicts what this product says
// about itself. This module is preloaded before the test file and denies every outbound path
// the runtime offers.
//
// It is a preload rather than a sandbox primitive, so it stops ordinary code rather than a
// determined attacker who reloads the modules. It is the strongest thing available without a
// kernel-level boundary, and it is honest about being that.

import net from 'node:net';
import dns from 'node:dns';
import tls from 'node:tls';
import http from 'node:http';
import https from 'node:https';
import dgram from 'node:dgram';

const denied = (what) => {
  const error = new Error(`network access is denied while running generated tests (${what})`);
  error.code = 'ERR_ACCESS_DENIED';
  return error;
};

const block = (object, name, what) => {
  if (object && typeof object[name] === 'function') {
    object[name] = () => {
      throw denied(what);
    };
  }
};

// TCP
block(net.Socket.prototype, 'connect', 'net.Socket.connect');
block(net, 'connect', 'net.connect');
block(net, 'createConnection', 'net.createConnection');

// TLS
block(tls, 'connect', 'tls.connect');

// DNS
block(dns, 'lookup', 'dns.lookup');
block(dns, 'resolve', 'dns.resolve');
if (dns.promises) {
  block(dns.promises, 'lookup', 'dns.promises.lookup');
  block(dns.promises, 'resolve', 'dns.promises.resolve');
}

// HTTP and HTTPS clients. The server factories are left alone so a test may still exercise
// request handlers directly without binding a socket.
block(http, 'request', 'http.request');
block(http, 'get', 'http.get');
block(https, 'request', 'https.request');
block(https, 'get', 'https.get');

// UDP
block(dgram, 'createSocket', 'dgram.createSocket');

// The global fetch, which bypasses the modules above.
globalThis.fetch = async () => {
  throw denied('fetch');
};
