/**
 * The pin of the public key of a farglass server (ADR-060).
 *
 * farglassd keeps the key of its self-signed certificate when it makes a new
 * certificate. Thus the client pins the SHA-256 of the public key (the
 * SubjectPublicKeyInfo), not the SHA-256 of the certificate.
 */
import { X509Certificate, createHash } from "node:crypto";

/** The result of a check of a certificate against a pin. */
export type PinCheck = "match" | "new" | "changed";

/**
 * Returns the SHA-256 of the public key of the PEM certificate `pem`, in lowercase hex.
 * It is the same text as the output of {@link HOST_PIN_COMMAND} on the host.
 */
export function publicKeyPin(pem: string): string {
  const spki = new X509Certificate(pem).publicKey.export({ type: "spki", format: "der" });
  return createHash("sha256")
    .update(spki)
    .digest("hex");
}

/** The command that prints the pin of the self-signed certificate on the host. */
export const HOST_PIN_COMMAND =
  "sudo openssl x509 -in /var/lib/farglass/tls/self-signed-cert.pem -noout -pubkey | openssl pkey -pubin -outform der | sha256sum";

/** Compares the pin of a certificate with the stored pin. No stored pin gives "new". */
export function checkPin(stored: string | undefined, actual: string): PinCheck {
  if (!stored) return "new";
  return stored === actual ? "match" : "changed";
}
