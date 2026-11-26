import { describe, expect, it } from "vitest";
import { checkPin, publicKeyPin } from "./pin";

// Two certificates with one P-256 key, as farglassd makes them when it renews its self-signed certificate.
const FIRST = `-----BEGIN CERTIFICATE-----
MIIBbTCCAROgAwIBAgIUA/acTvM+X0xfhLxvxyfute2qsM4wCgYIKoZIzj0EAwIw
DDEKMAgGA1UEAwwBdDAeFw0yNjExMjYyMzA4MzZaFw0yNjExMjcyMzA4MzZaMAwx
CjAIBgNVBAMMAXQwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAATi2stzHE1l21Xs
2TIkc7q9jzziMM2VnQ2ogruLr3JPTLsUDhGatFx4qYcS1U02bJNyTzf0BoMGPlim
arGe0Ol0o1MwUTAdBgNVHQ4EFgQUNNgpnMDAwI9CC+IaAFdGUNaKBuEwHwYDVR0j
BBgwFoAUNNgpnMDAwI9CC+IaAFdGUNaKBuEwDwYDVR0TAQH/BAUwAwEB/zAKBggq
hkjOPQQDAgNIADBFAiASmdgftDdE/2MSzSzydqLS177mB3wJdD6o9/CX6rkabgIh
AJKTz/SX7l7dprFlPuolKxGCEsA0hHDhFuusIjfe0gxh
-----END CERTIFICATE-----
`;
const RENEWED = `-----BEGIN CERTIFICATE-----
MIIBbzCCARWgAwIBAgIUBagyc0COY2VErZ5FR5TOgVgKhoAwCgYIKoZIzj0EAwIw
DTELMAkGA1UEAwwCdDIwHhcNMjYxMTI2MjMwODM2WhcNMjYxMTI4MjMwODM2WjAN
MQswCQYDVQQDDAJ0MjBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABOLay3McTWXb
VezZMiRzur2PPOIwzZWdDaiCu4uvck9MuxQOEZq0XHiphxLVTTZsk3JPN/QGgwY+
WKZqsZ7Q6XSjUzBRMB0GA1UdDgQWBBQ02CmcwMDAj0IL4hoAV0ZQ1ooG4TAfBgNV
HSMEGDAWgBQ02CmcwMDAj0IL4hoAV0ZQ1ooG4TAPBgNVHRMBAf8EBTADAQH/MAoG
CCqGSM49BAMCA0gAMEUCIQC3yGH0a9IqjtWiPNPu7lkSInHIlETLnzS4x7GdGOsD
vgIgMVPqEuGd5aD6VPN8CdDqCu2uaqx53AVUxYUARkqFhFE=
-----END CERTIFICATE-----
`;
// The output of HOST_PIN_COMMAND for the key of the two certificates.
const PIN = "f34bdbf5d72c870ee3c568860445d8541c3c3447768d2645d0e6e8f806cc26bf";

describe("publicKeyPin", () => {
  it("gives the SHA-256 of the public key, as the command on the host", () => {
    expect(publicKeyPin(FIRST)).toBe(PIN);
  });
  it("gives the same pin for a new certificate with the same key", () => {
    expect(publicKeyPin(RENEWED)).toBe(PIN);
  });
});

describe("checkPin", () => {
  it("compares with the kept pin", () => {
    expect(checkPin(undefined, PIN)).toBe("new");
    expect(checkPin(PIN, PIN)).toBe("match");
    expect(checkPin("00", PIN)).toBe("changed");
  });
});
